import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, join, resolve, sep } from 'node:path';
const fixture = process.argv[2] ?? 'grid-return';
if (!['grid-return', 'copy-resources', 'link-boards', 'video-download', 'categories', 'workflow-tools', 'pointer-moves', 'wide-preview', 'library-export'].includes(fixture)) throw Error('Unknown browser test fixture');
const port = fixture === "wide-preview" ? "1422" : "1421";
const testUrl = `http://127.0.0.1:${port}/tests/${fixture}.html`;

const chrome = [process.env.ProgramFiles, process.env['ProgramFiles(x86)'], process.env.LOCALAPPDATA]
  .filter(Boolean).map(root => join(root, 'Google/Chrome/Application/chrome.exe')).find(existsSync);
if (!chrome) throw Error('Chrome is required for the isolated headless check.');
const server = spawn(process.execPath, ['node_modules/vite/bin/vite.js', '--host', '127.0.0.1', '--port', port, '--strictPort'], { stdio: 'pipe', windowsHide: true });
let serverError = '';
server.stderr.on('data', chunk => { serverError += chunk; });
let started = false;
try {
  for (let n = 0; n < 100; n++) {
    if (server.exitCode !== null) throw Error(serverError || 'Test server stopped');
    try { if ((await fetch(testUrl)).ok) { started = true; break; } } catch {}
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  if (!started) throw Error('Test server did not start');
  for (const dimensions of (fixture === 'wide-preview' ? ['3440,1440', '1920,1080', '1600,940', '1599,940'] : ['1440,940', '960,700'])) {
    const profile = await mkdtemp(join(tmpdir(), 'design-grid-check-'));
    try {
      // An isolated headless process uses real browser frames and keyboard events.
      // It never attaches to, opens or focuses the user's browser windows.
      const child = spawn(chrome, ['--headless', '--disable-gpu', '--disable-extensions', '--disable-background-networking', '--no-first-run', '--no-default-browser-check', '--remote-debugging-pipe', `--user-data-dir=${profile}`], { stdio: ['ignore', 'pipe', 'pipe', 'pipe', 'pipe'], windowsHide: true });
      let sequence = 0, buffered = '', result;
      const pending = new Map();
      child.stdio[4].on('data', chunk => {
        buffered += chunk;
        let end;
        while ((end = buffered.indexOf('\0')) >= 0) {
          const message = JSON.parse(buffered.slice(0, end));
          buffered = buffered.slice(end + 1);
          const callback = pending.get(message.id);
          if (callback) { pending.delete(message.id); callback(message); }
        }
      });
      const call = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
        const id = ++sequence;
        const timer = setTimeout(() => { pending.delete(id); reject(Error(`Browser command timed out: ${method}`)); }, method === 'Page.navigate' ? 30000 : 10000);
        pending.set(id, message => {
          clearTimeout(timer);
          if (message.error) reject(Error(JSON.stringify(message.error)));
          else resolve(message.result);
        });
        child.stdio[3].write(JSON.stringify({ id, method, params, sessionId }) + '\0');
      });
      try {
        await call('Browser.getVersion');
        const { targetId } = await call('Target.createTarget', { url: 'about:blank' });
        const { sessionId } = await call('Target.attachToTarget', { targetId, flatten: true });
        const [width, height] = dimensions.split(',').map(Number);
        await call('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false }, sessionId);
        await call('Page.bringToFront', {}, sessionId);
        await call('Page.navigate', { url: testUrl }, sessionId);
        let sentEnter = false;
        let lastPointerAction=0;
        let lastResizeAction=0;
        const deadline = Date.now() + 60000;
        while (Date.now() < deadline) {
          const { result: state } = await call('Runtime.evaluate', { expression: '({ report: document.getElementById("check-result")?.textContent, enter: !!window.__awaitNativeEnter, pointer: window.__pointerAction, resize: window.__resizeAction })', returnByValue: true }, sessionId);
          if (state.value?.resize?.id > lastResizeAction) {
            const action = state.value.resize; lastResizeAction = action.id;
            await call('Emulation.setDeviceMetricsOverride', { width: action.width, height, deviceScaleFactor: 1, mobile: false }, sessionId);
            await call('Runtime.evaluate', { expression: `window.__resizeActionDone=${action.id}` }, sessionId);
          }
          if(state.value?.pointer?.id>lastPointerAction){
            const action=state.value.pointer;lastPointerAction=action.id;
            await call('Input.dispatchMouseEvent',{type:'mouseMoved',x:action.from.x,y:action.from.y},sessionId);
            await call('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',buttons:1,clickCount:1,x:action.from.x,y:action.from.y},sessionId);
            if(!action.click){
              await call('Input.dispatchMouseEvent',{type:'mouseMoved',buttons:1,x:action.from.x+12,y:action.from.y+8},sessionId);
              await call('Runtime.evaluate',{expression:'window.__duringPointerDrag?.()'},sessionId);
              await call('Input.dispatchMouseEvent',{type:'mouseMoved',buttons:1,x:action.to.x,y:action.to.y,modifiers:action.ctrl?2:0},sessionId);
              if(action.pause)await new Promise(resolve=>setTimeout(resolve,action.pause));
              await call('Runtime.evaluate',{expression:'window.__checkPointerDrag?.()'},sessionId);
              if(action.escape){
                await call('Input.dispatchKeyEvent',{type:'keyDown',key:'Escape',code:'Escape',windowsVirtualKeyCode:27},sessionId);
                await call('Input.dispatchKeyEvent',{type:'keyUp',key:'Escape',code:'Escape',windowsVirtualKeyCode:27},sessionId);
              }
            }
            await call('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,x:action.to.x,y:action.to.y,modifiers:action.ctrl?2:0},sessionId);
            await call('Runtime.evaluate',{expression:`window.__pointerActionDone=${action.id}`},sessionId);
          }
          if (state.value?.enter && !sentEnter) {
            sentEnter = true;
            await call('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Enter', code: 'Enter', text: '\r', windowsVirtualKeyCode: 13 }, sessionId);
            await call('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13 }, sessionId);
          }
          if (state.value?.report && state.value.report !== 'pending') { result = JSON.parse(state.value.report); break; }
          await new Promise(resolve => setTimeout(resolve, 100));
        }
        if (!result) throw Error('Headless check timed out');
      } finally {
        if (child.exitCode === null) {
          const exited = new Promise(resolve => child.once('exit', resolve));
          child.kill();
          await exited;
        }
      }
      console.log(JSON.stringify({ dimensions, ...result }, null, 2));
      if (!result.ok) throw Error(result.error);
    } finally {
      const target = resolve(profile);
      if (!target.startsWith(resolve(tmpdir()) + sep) || !basename(target).startsWith('design-grid-check-')) throw Error('Unexpected temporary profile path');
      await rm(target, { recursive: true, force: true, maxRetries: 5, retryDelay: 300 });
    }
  }
} finally { server.kill(); }
