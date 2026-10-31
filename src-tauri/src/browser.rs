use crate::db::{err, valid_url, Result};
use std::{path::PathBuf, process::Command};

pub fn chrome_path() -> Result<PathBuf> {
    ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(|root| PathBuf::from(root).join("Google/Chrome/Application/chrome.exe"))
        .find(|path| path.is_file())
        .ok_or_else(|| "Google Chrome could not be found. Install Chrome to open links.".into())
}

pub fn open(url: &str) -> Result<()> {
    let url = valid_url(url)?;
    let mut command = Command::new(chrome_path()?);
    command.args(["--new-tab", url]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.spawn().map_err(err)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn external_links_accept_only_web_urls_and_trim_pasted_spacing() {
        assert_eq!(
            crate::db::valid_url(" https://www.behance.net/galleries/ui-ux\u{a0}").unwrap(),
            "https://www.behance.net/galleries/ui-ux"
        );
        for url in [
            "javascript:alert(1)",
            "file:///C:/test",
            "--incognito",
            "https://example.com/ bad",
        ] {
            assert!(crate::db::valid_url(url).is_err());
        }
    }
}
