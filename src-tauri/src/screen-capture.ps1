param([string]$OutputPath, [double]$Ratio = 1.3333333333, [switch]$CheckOnly, [switch]$SelfTest)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -ReferencedAssemblies System.Windows.Forms,System.Drawing -TypeDefinition @'
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Windows.Forms;
using System.Runtime.InteropServices;
public sealed class DesignSnip : Form {
    [DllImport("user32.dll")] static extern bool SetProcessDPIAware();
    Bitmap desktop; Point start; Rectangle selection, beforeMove, alignmentScreen; bool dragging, moving, alignedX, alignedY; double ratio; string output;
    static Rectangle AlignToCenter(Rectangle box, Rectangle screen, bool bypass, out bool horizontal, out bool vertical) {
        int x = screen.X + (int)Math.Round((screen.Width-box.Width)/2.0);
        int y = screen.Y + (int)Math.Round((screen.Height-box.Height)/2.0);
        horizontal = !bypass && box.Width <= screen.Width && Math.Abs(box.X-x) <= 8;
        vertical = !bypass && box.Height <= screen.Height && Math.Abs(box.Y-y) <= 8;
        return new Rectangle(horizontal ? x : box.X, vertical ? y : box.Y, box.Width, box.Height);
    }
    public static void Run(string path, double aspect) {
        SetProcessDPIAware();
        Application.EnableVisualStyles();
        using (var form = new DesignSnip(path, aspect)) Application.Run(form);
    }
    public static void Test(string path) {
        bool ax, ay;
        var monitor = new Rectangle(100,50,1200,900);
        var nearCenter = new Rectangle(507,344,400,300);
        var aligned = AlignToCenter(nearCenter,monitor,false,out ax,out ay);
        if (!ax || !ay || aligned != new Rectangle(500,350,400,300)) throw new Exception("Both centre axes must snap without resizing");
        if (AlignToCenter(nearCenter,monitor,true,out ax,out ay) != nearCenter || ax || ay) throw new Exception("Alt must bypass centering");
        var offCenter = new Rectangle(700,344,400,300);
        aligned = AlignToCenter(offCenter,monitor,false,out ax,out ay);
        if (ax || !ay || aligned.X != 700 || aligned.Y != 350) throw new Exception("Each centre axis must snap independently");
        var distant = new Rectangle(700,500,400,300);
        if (AlignToCenter(distant,monitor,false,out ax,out ay) != distant || ax || ay) throw new Exception("Distant boxes must move freely");
        aligned = AlignToCenter(new Rectangle(-1305,295,400,300),new Rectangle(-1600,0,1000,900),false,out ax,out ay);
        if (!ax || !ay || aligned.X != -1300 || aligned.Y != 300) throw new Exception("Other monitor origins must be respected");
        foreach (double aspect in new double[] {4.0/3, 1, 16.0/9, 3.0/4, 0}) {
            using (var form = new DesignSnip(path, aspect, true)) {
                form.OnMouseDown(new MouseEventArgs(MouseButtons.Left, 1, 500, 400, 0));
                form.OnMouseMove(new MouseEventArgs(MouseButtons.Left, 0, 100, 100, 0));
                form.OnMouseUp(new MouseEventArgs(MouseButtons.Left, 1, 100, 100, 0));
                if (System.IO.File.Exists(path)) throw new Exception("Mouse release must not save");
                Rectangle drawn = form.selection;
                int x = drawn.X + drawn.Width/2, y = drawn.Y + drawn.Height/2;
                form.OnMouseDown(new MouseEventArgs(MouseButtons.Left, 1, x, y, 0));
                form.OnMouseMove(new MouseEventArgs(MouseButtons.Left, 0, x+20, y+10, 0));
                form.OnMouseUp(new MouseEventArgs(MouseButtons.Left, 1, x+20, y+10, 0));
                Rectangle moved = form.selection;
                if (moved.Size != drawn.Size || moved.X != drawn.X+20 || moved.Y != drawn.Y+10) throw new Exception("Reposition changed size or failed to move");
                x = moved.X+moved.Width/2; y = moved.Y+moved.Height/2;
                form.OnMouseDown(new MouseEventArgs(MouseButtons.Left, 1, x, y, 0));
                form.OnMouseMove(new MouseEventArgs(MouseButtons.Left, 0, -9999, -9999, 0));
                form.OnMouseUp(new MouseEventArgs(MouseButtons.Left, 1, -9999, -9999, 0));
                moved = form.selection;
                if (moved.Location != Point.Empty || moved.Size != drawn.Size) throw new Exception("Move must stay inside screen");
                if (System.IO.File.Exists(path)) throw new Exception("Reposition must not save");
                using (var preview = new Bitmap(form.ClientSize.Width,form.ClientSize.Height))
                using (var graphics = Graphics.FromImage(preview)) form.OnPaint(new PaintEventArgs(graphics,form.ClientRectangle));
                form.OnKeyDown(new KeyEventArgs(Keys.Enter));
            }
            using (var result = new Bitmap(path)) {
                if (aspect > 0 && Math.Abs((double)result.Width/result.Height-aspect) > 0.01) throw new Exception("Capture ratio failed");
                if (result.GetPixel(0,0).ToArgb() != Color.Coral.ToArgb()) throw new Exception("Captured pixels failed");
                if (result.GetPixel(result.Width/2,result.Height/2).ToArgb() != Color.Coral.ToArgb()) throw new Exception("Centre guides must not appear in the saved image");
                if (aspect == 0 && (result.Width != 400 || result.Height != 300)) throw new Exception("Free capture failed");
            }
            System.IO.File.Delete(path);
        }
        using (var form = new DesignSnip(path, 4.0/3, true)) {
            form.OnMouseDown(new MouseEventArgs(MouseButtons.Left, 1, 20, 20, 0));
            form.OnMouseMove(new MouseEventArgs(MouseButtons.Left, 0, 220, 170, 0));
            form.OnMouseUp(new MouseEventArgs(MouseButtons.Left, 1, 220, 170, 0));
            form.OnKeyDown(new KeyEventArgs(Keys.Escape));
            if (System.IO.File.Exists(path)) throw new Exception("Escape must not save");
        }
        using (var form = new DesignSnip(path, 1, true)) {
            form.OnKeyDown(new KeyEventArgs(Keys.Enter));
            if (System.IO.File.Exists(path)) throw new Exception("Empty selection must not save");
            form.OnMouseDown(new MouseEventArgs(MouseButtons.Left, 1, 20, 20, 0));
            form.OnMouseMove(new MouseEventArgs(MouseButtons.Left, 0, 220, 220, 0));
            form.OnMouseUp(new MouseEventArgs(MouseButtons.Left, 1, 220, 220, 0));
            form.OnMouseDoubleClick(new MouseEventArgs(MouseButtons.Left, 2, 100, 100, 0));
            if (!System.IO.File.Exists(path)) throw new Exception("Double-click must save");
        }
        System.IO.File.Delete(path);
    }
    DesignSnip(string path, double aspect, bool synthetic = false) {
        output = path; ratio = aspect;
        var bounds = SystemInformation.VirtualScreen;
        desktop = new Bitmap(bounds.Width, bounds.Height);
        using (var g = Graphics.FromImage(desktop)) {
            if (synthetic) g.Clear(Color.Coral);
            else g.CopyFromScreen(bounds.Location, Point.Empty, bounds.Size);
        }
        AutoScaleMode = AutoScaleMode.None; FormBorderStyle = FormBorderStyle.None;
        StartPosition = FormStartPosition.Manual; Bounds = bounds;
        TopMost = true; ShowInTaskbar = false; KeyPreview = true;
        DoubleBuffered = true; Cursor = Cursors.Cross;
        KeyDown += (s,e) => {
            if (e.KeyCode == Keys.Escape) Close();
            if (e.KeyCode == Keys.Enter && !dragging) { e.Handled = true; e.SuppressKeyPress = true; SaveSelection(); }
        };
        MouseDoubleClick += (s,e) => { if (e.Button == MouseButtons.Left && selection.Contains(e.Location)) SaveSelection(); };
        MouseDown += (s,e) => {
            if (e.Button == MouseButtons.Right) { Close(); return; }
            if (e.Button != MouseButtons.Left) return;
            start = e.Location; moving = selection.Contains(e.Location); beforeMove = selection;
            alignedX = alignedY = false;
            if (!moving) selection = Rectangle.Empty;
            dragging = true; Capture = true; Invalidate();
        };
        MouseMove += (s,e) => {
            if (!dragging) { Cursor = selection.Contains(e.Location) ? Cursors.SizeAll : Cursors.Cross; return; }
            if (moving) {
                selection = new Rectangle(Math.Max(0, Math.Min(ClientSize.Width-beforeMove.Width, beforeMove.X+e.X-start.X)),
                    Math.Max(0, Math.Min(ClientSize.Height-beforeMove.Height, beforeMove.Y+e.Y-start.Y)), beforeMove.Width, beforeMove.Height);
                var screen = Screen.FromPoint(new Point(Bounds.X+selection.X+selection.Width/2,Bounds.Y+selection.Y+selection.Height/2)).Bounds;
                alignmentScreen = new Rectangle(screen.X-Bounds.X,screen.Y-Bounds.Y,screen.Width,screen.Height);
                selection = AlignToCenter(selection,alignmentScreen,(ModifierKeys & Keys.Alt) != 0,out alignedX,out alignedY);
                Invalidate(); return;
            }
            double w = Math.Abs(e.X-start.X), h = Math.Abs(e.Y-start.Y);
            if (ratio > 0) {
                w = Math.Max(w, h*ratio); h = w/ratio;
                double maxW = e.X >= start.X ? ClientSize.Width-start.X : start.X;
                double maxH = e.Y >= start.Y ? ClientSize.Height-start.Y : start.Y;
                double scale = Math.Min(1, Math.Min(maxW/Math.Max(w,1), maxH/Math.Max(h,1)));
                w *= scale; h *= scale;
            }
            int width = (int)Math.Round(w), height = (int)Math.Round(h);
            selection = new Rectangle(e.X >= start.X ? start.X : start.X-width,
                e.Y >= start.Y ? start.Y : start.Y-height, width, height);
            Invalidate();
        };
        MouseUp += (s,e) => {
            if (!dragging || e.Button != MouseButtons.Left) return;
            dragging = false; Capture = false;
            if (selection.Width < 12 || selection.Height < 12) selection = Rectangle.Empty;
            Cursor = selection.Contains(e.Location) ? Cursors.SizeAll : Cursors.Cross;
            Invalidate();
        };
    }
    void SaveSelection() {
        if (selection.Width < 12 || selection.Height < 12) return;
        dragging = false; Capture = false;
        try {
            using (var crop = desktop.Clone(selection, PixelFormat.Format32bppArgb)) crop.Save(output, ImageFormat.Png);
        } catch (Exception ex) { Console.Error.WriteLine(ex.Message); }
        Close();
    }
    protected override void OnPaint(PaintEventArgs e) {
        e.Graphics.DrawImageUnscaled(desktop, 0, 0);
        using (var dim = new SolidBrush(Color.FromArgb(115, 0, 0, 0))) e.Graphics.FillRectangle(dim, ClientRectangle);
        if (selection.Width > 0 && selection.Height > 0) {
            e.Graphics.DrawImage(desktop, selection, selection, GraphicsUnit.Pixel);
            using (var pen = new Pen(Color.White, 2)) e.Graphics.DrawRectangle(pen, selection);
            float cx = selection.X+selection.Width/2f, cy = selection.Y+selection.Height/2f;
            // Thin, quiet guides stay inside the crop; only the saved desktop pixels are exported.
            using (var shadow = new Pen(Color.FromArgb(75,0,0,0),1))
            using (var guide = new Pen(Color.FromArgb(115,255,255,255),1)) {
                shadow.DashStyle = guide.DashStyle = System.Drawing.Drawing2D.DashStyle.Dot;
                e.Graphics.DrawLine(shadow,selection.Left,cy+1,selection.Right,cy+1);
                e.Graphics.DrawLine(shadow,cx+1,selection.Top,cx+1,selection.Bottom);
                e.Graphics.DrawLine(guide,selection.Left,cy,selection.Right,cy);
                e.Graphics.DrawLine(guide,cx,selection.Top,cx,selection.Bottom);
            }
            using (var shadow = new Pen(Color.FromArgb(100,0,0,0),3))
            using (var cross = new Pen(Color.FromArgb(195,255,255,255),1)) {
                e.Graphics.DrawLine(shadow,cx-5,cy,cx+5,cy); e.Graphics.DrawLine(shadow,cx,cy-5,cx,cy+5);
                e.Graphics.DrawLine(cross,cx-5,cy,cx+5,cy); e.Graphics.DrawLine(cross,cx,cy-5,cx,cy+5);
            }
            if (dragging && moving) using (var align = new Pen(Color.FromArgb(165,170,220,185),1)) {
                if (alignedX) e.Graphics.DrawLine(align,cx,alignmentScreen.Top,cx,alignmentScreen.Bottom);
                if (alignedY) e.Graphics.DrawLine(align,alignmentScreen.Left,cy,alignmentScreen.Right,cy);
            }
        }
        if (!dragging) {
            string help = selection.IsEmpty ? "Drag to size the box   |   Esc / right-click to cancel" : "Drag to move   |   Enter / double-click saves   |   Alt skips snap   |   Esc cancels";
            var screen = Screen.FromPoint(Cursor.Position).Bounds;
            int helpWidth = Math.Min(screen.Width-16, (int)Math.Ceiling(e.Graphics.MeasureString(help, SystemFonts.MessageBoxFont).Width)+24);
            int x = screen.X-Bounds.X+(screen.Width-helpWidth)/2;
            int y = screen.Bottom-Bounds.Y-42;
            if (selection.IntersectsWith(new Rectangle(x,y,helpWidth,30))) y = screen.Top-Bounds.Y+12;
            using (var bg = new SolidBrush(Color.FromArgb(210, 25, 25, 25))) e.Graphics.FillRectangle(bg, x, y, helpWidth, 30);
            e.Graphics.DrawString(help, SystemFonts.MessageBoxFont, Brushes.White, x+10, y+8);
        }
    }
    protected override void Dispose(bool disposing) { if (disposing && desktop != null) desktop.Dispose(); base.Dispose(disposing); }
}
'@
if ($SelfTest) { [DesignSnip]::Test($OutputPath); Write-Output 'Capture checks passed: centre snapping on each axis, Alt bypass, multiple monitor origins, free movement, all ratios, reposition, bounds, save and cancel. Saved pixels contain no guides.' }
elseif (!$CheckOnly) { [DesignSnip]::Run($OutputPath, $Ratio) }
