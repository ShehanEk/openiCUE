//! Exact text of what Setup registers. Pure, so it is tested.
use std::path::Path;

pub const TASK_NAME: &str = r"\openiCUE\Restore";
pub const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\openiCUE";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Text(String),
    Number(u32),
}

pub fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Runs `engine --restore` at any user's logon, elevated for admins, with
/// no time limit (a custom effect keeps running).
pub fn task_xml(engine: &Path) -> String {
    let command = xml_escape(&engine.to_string_lossy());
    format!(r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>Restores the openiCUE lighting effect at logon.</Description></RegistrationInfo>
  <Triggers><LogonTrigger><Enabled>true</Enabled></LogonTrigger></Triggers>
  <Principals><Principal id="Users"><GroupId>S-1-5-32-545</GroupId><RunLevel>HighestAvailable</RunLevel></Principal></Principals>
  <Settings>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Users"><Exec><Command>{command}</Command><Arguments>--restore</Arguments></Exec></Actions>
</Task>
"#)
}

pub fn uninstall_values(dir: &Path, size_kb: u32) -> Vec<(&'static str, Value)> {
    let dir = dir.to_string_lossy().to_string();
    vec![
        ("DisplayName", Value::Text("openiCUE".into())),
        ("DisplayVersion", Value::Text(env!("CARGO_PKG_VERSION").into())),
        ("Publisher", Value::Text("openiCUE".into())),
        ("DisplayIcon", Value::Text(format!(r"{dir}\openicue.exe"))),
        ("InstallLocation", Value::Text(dir.clone())),
        ("UninstallString", Value::Text(format!(r#""{dir}\setup.exe" --uninstall"#))),
        ("NoModify", Value::Number(1)),
        ("NoRepair", Value::Number(1)),
        ("EstimatedSize", Value::Number(size_kb)),
    ]
}

/// True when `path` is somewhere below `dir`. Windows paths ignore case, and
/// a `\\?\` prefix on either side is ignored.
pub fn is_inside(path: &Path, dir: &Path) -> bool {
    fn parts(p: &Path) -> Vec<String> {
        let text = p.to_string_lossy();
        let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
        text.split(['\\', '/']).filter(|s| !s.is_empty()).map(str::to_lowercase).collect()
    }
    let (path, dir) = (parts(path), parts(dir));
    path.len() > dir.len() && path.starts_with(&dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_inside_ignores_case_and_needs_whole_folder_names() {
        let dir = Path::new(r"C:\Program Files\openiCUE");
        assert!(is_inside(Path::new(r"C:\Program Files\openiCUE\setup.exe"), dir));
        assert!(is_inside(Path::new(r"c:\program files\OPENICUE\setup.exe"), dir));
        assert!(is_inside(Path::new(r"\\?\C:\Program Files\openiCUE\setup.exe"), dir), "verbatim prefix");
        assert!(is_inside(Path::new(r"C:\Program Files\openiCUE\sub\x.exe"), dir));
        assert!(!is_inside(Path::new(r"C:\Program Files\openiCUE2\setup.exe"), dir), "a sibling is not inside");
        assert!(!is_inside(Path::new(r"C:\Users\me\Downloads\openiCUE-Setup.exe"), dir));
        assert!(!is_inside(Path::new(r"C:\Program Files\openiCUE"), dir), "the folder itself is not a file in it");
        assert!(!is_inside(Path::new(r"D:\Program Files\openiCUE\setup.exe"), dir));
    }
    use std::path::Path;

    #[test]
    fn task_runs_the_engine_restore_at_logon_with_highest_privileges() {
        let xml = task_xml(Path::new(r"C:\Program Files\openiCUE\openicue-engine.exe"));
        assert!(xml.contains(r"<Command>C:\Program Files\openiCUE\openicue-engine.exe</Command>"));
        assert!(xml.contains("<Arguments>--restore</Arguments>"));
        assert!(xml.contains("<LogonTrigger>"));
        assert!(xml.contains("<GroupId>S-1-5-32-545</GroupId>"));
        assert!(xml.contains("<RunLevel>HighestAvailable</RunLevel>"));
        assert!(xml.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
        assert!(xml.contains("<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>"));
    }

    #[test]
    fn xml_escapes_paths() {
        assert_eq!(xml_escape(r"C:\A&B<C>"), r"C:\A&amp;B&lt;C&gt;");
        let xml = task_xml(Path::new(r"C:\A&B\openicue-engine.exe"));
        assert!(xml.contains(r"C:\A&amp;B\openicue-engine.exe"));
    }

    #[test]
    fn uninstall_entry_points_at_setup_with_the_uninstall_flag() {
        let values = uninstall_values(Path::new(r"C:\Program Files\openiCUE"), 1200);
        let get = |name: &str| values.iter().find(|(n, _)| *n == name).map(|(_, v)| v.clone());
        assert_eq!(get("DisplayName"), Some(Value::Text("openiCUE".into())));
        assert_eq!(get("UninstallString"), Some(Value::Text(r#""C:\Program Files\openiCUE\setup.exe" --uninstall"#.into())));
        assert_eq!(get("InstallLocation"), Some(Value::Text(r"C:\Program Files\openiCUE".into())));
        assert_eq!(get("NoModify"), Some(Value::Number(1)));
        assert_eq!(get("NoRepair"), Some(Value::Number(1)));
        assert_eq!(get("EstimatedSize"), Some(Value::Number(1200)));
    }
}
