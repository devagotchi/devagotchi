use super::line::route;
use super::*;
use std::collections::HashMap;

#[derive(Default)]
struct Fake {
    vars: HashMap<&'static str, String>,
    files: HashMap<String, (bool, u64, Vec<u8>)>,
    dirs: Vec<String>,
}

impl Fake {
    fn var(mut self, name: &'static str, value: &str) -> Self {
        self.vars.insert(name, value.to_owned());
        self
    }
    fn file(mut self, path: &str, executable: bool, head: &[u8]) -> Self {
        self.files.insert(path.to_owned(), (executable, head.len().max(1) as u64, head.to_vec()));
        self
    }
    fn alias(mut self, path: &str) -> Self {
        self.files.insert(path.to_owned(), (false, 0, Vec::new()));
        self
    }
}

impl Host for Fake {
    fn var(&self, name: &str) -> Option<String> {
        self.vars.get(name).cloned()
    }
    fn kind(&self, path: &str) -> Kind {
        if self.dirs.iter().any(|dir| dir == path) {
            return Kind::Directory;
        }
        match self.files.get(path) {
            Some((executable, size, _)) => Kind::File { executable: *executable, size: *size },
            None => Kind::Missing,
        }
    }
    fn head(&self, path: &str) -> Vec<u8> {
        self.files.get(path).map(|(_, _, head)| head.clone()).unwrap_or_default()
    }
}

const ELF: &[u8] = b"\x7fELF\x02\x01";

fn linux() -> Fake {
    Fake::default()
        .var("PATH", "/usr/local/bin:/usr/bin:/bin")
        .file("/usr/bin/python3", true, ELF)
        .file("/usr/bin/node", true, ELF)
        .file("/bin/sh", true, ELF)
        .file("/root/.cargo/bin/cargo", true, ELF)
}

#[test]
fn a_bare_name_is_found_on_path_and_spawned_by_the_path_found() {
    let launch = resolve("cargo", "/w", Os::Unix, &linux().var("PATH", "/root/.cargo/bin:/usr/bin")).unwrap();
    assert_eq!(launch.program, "/root/.cargo/bin/cargo");
    assert!(launch.lead.is_empty());
    assert!(launch.how.contains("found on PATH in /root/.cargo/bin"), "{}", launch.how);
}

#[test]
fn a_missing_name_says_how_many_directories_were_searched() {
    let error = resolve("python9", "/w", Os::Unix, &linux()).unwrap_err();
    assert_eq!(error.code, "program_not_found");
    assert!(error.message.contains("3 directories"), "{}", error.message);
}

#[test]
fn a_python_script_without_execute_permission_runs_with_python3() {
    let host = linux().file("/w/hello.py", false, b"print('hi')\n");
    let launch = resolve("hello.py", "/w", Os::Unix, &host).unwrap_err();
    // A bare name is looked up on PATH, never in the working directory.
    assert_eq!(launch.code, "program_not_found");
    let launch = resolve("./hello.py", "/w", Os::Unix, &host).unwrap();
    assert_eq!(launch.program, "/usr/bin/python3");
    assert_eq!(launch.lead, ["/w/hello.py"]);
    assert!(launch.how.contains("ends in .py"), "{}", launch.how);
}

#[test]
fn an_executable_script_with_a_shebang_runs_itself() {
    let host = linux().file("/w/tool", true, b"#!/usr/bin/env python3\n");
    let launch = resolve("/w/tool", "/w", Os::Unix, &host).unwrap();
    assert_eq!(launch.program, "/w/tool");
    assert!(launch.lead.is_empty());
}

#[test]
fn a_shebang_names_the_interpreter_when_the_file_is_not_executable() {
    let host = linux().file("/w/tool", false, b"#!/usr/bin/env -S python3 -u\nprint(1)\n");
    let launch = resolve("/w/tool", "/w", Os::Unix, &host).unwrap();
    assert_eq!(launch.program, "/usr/bin/python3");
    assert_eq!(launch.lead, ["-u", "/w/tool"]);
    let host = linux().file("/w/old", false, b"#!/opt/gone/python\n");
    assert_eq!(resolve("/w/old", "/w", Os::Unix, &host).unwrap_err().code, "interpreter_not_found");
}

#[test]
fn a_file_with_no_way_to_run_it_says_what_would_make_it_run() {
    let host = linux().file("/w/notes.txt", false, b"hello");
    let error = resolve("/w/notes.txt", "/w", Os::Unix, &host).unwrap_err();
    assert_eq!(error.code, "not_executable");
    assert!(error.message.contains("chmod +x"));
}

#[test]
fn a_directory_or_a_missing_path_is_named() {
    let mut host = linux();
    host.dirs.push("/w/dir".into());
    assert_eq!(resolve("/w/dir", "/w", Os::Unix, &host).unwrap_err().code, "invalid_request");
    assert_eq!(resolve("/w/none", "/w", Os::Unix, &host).unwrap_err().code, "path_not_found");
}

fn windows() -> Fake {
    Fake::default()
        .var("PATH", "C:\\Windows\\system32;C:\\Users\\h\\AppData\\Local\\Microsoft\\WindowsApps;C:\\Program Files\\nodejs")
        .var("PATHEXT", ".COM;.EXE;.BAT;.CMD;.VBS;.JS")
        .var("SystemRoot", "C:\\Windows")
        .var("LOCALAPPDATA", "C:\\Users\\h\\AppData\\Local")
        .file("C:\\Program Files\\nodejs\\node.exe", false, b"MZ")
        .alias("C:\\Users\\h\\AppData\\Local\\Microsoft\\WindowsApps\\python.exe")
        .alias("C:\\Users\\h\\AppData\\Local\\Microsoft\\WindowsApps\\python3.exe")
}

#[test]
fn windows_never_treats_the_store_alias_as_python() {
    let host = windows().file("C:\\w\\build.py", false, b"print(1)");
    let error = resolve("C:\\w\\build.py", "C:\\w", Os::Windows, &host).unwrap_err();
    assert_eq!(error.code, "interpreter_not_found");
    assert!(error.message.contains("Microsoft Store alias"), "{}", error.message);
    assert!(error.message.contains("no Python launcher"), "{}", error.message);
}

#[test]
fn windows_runs_a_py_with_the_python_launcher() {
    let host = windows().file("C:\\w\\build.py", false, b"print(1)").file("C:\\Windows\\py.exe", false, b"MZ");
    let launch = resolve("build.py", "C:\\w", Os::Windows, &host);
    // A bare name is a PATH lookup; .py is not in PATHEXT.
    assert_eq!(launch.unwrap_err().code, "program_not_found");
    let launch = resolve(".\\build.py", "C:\\w", Os::Windows, &host).unwrap();
    assert_eq!(launch.program, "C:\\Windows\\py.exe");
    assert_eq!(launch.lead, ["C:\\w\\build.py"]);
    assert!(launch.how.contains("Python launcher"), "{}", launch.how);
}

#[test]
fn windows_finds_a_program_by_pathext_and_runs_ps1_with_powershell() {
    let host = windows().file("C:\\w\\setup.ps1", false, b"Write-Host 1");
    let launch = resolve("node", "C:\\w", Os::Windows, &host).unwrap();
    assert_eq!(launch.program, "C:\\Program Files\\nodejs\\node.exe");
    let launch = resolve("C:\\w\\setup.ps1", "C:\\w", Os::Windows, &host).unwrap();
    assert_eq!(launch.program, "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe");
    assert_eq!(launch.lead.last().unwrap(), "C:\\w\\setup.ps1");
    assert!(launch.lead.contains(&"-File".to_owned()));
}

#[test]
fn windows_refuses_a_file_it_has_no_interpreter_for() {
    let host = windows().file("C:\\w\\notes.txt", false, b"x");
    let error = resolve("C:\\w\\notes.txt", "C:\\w", Os::Windows, &host).unwrap_err();
    assert_eq!(error.code, "not_executable");
    assert!(error.message.contains(".txt"));
}

#[test]
fn a_unix_line_that_starts_with_a_non_executable_script_names_its_interpreter() {
    let host = linux().file("/w/hello.py", false, b"print(1)\n").file("/w/run.sh", true, b"#!/bin/sh\n");
    let (line, how) = route("./hello.py --fast", "/w", Os::Unix, &host).unwrap().unwrap();
    assert_eq!(line, "'/usr/bin/python3' ./hello.py --fast");
    assert!(how.contains(".py"));
    assert_eq!(route("./run.sh", "/w", Os::Unix, &host).unwrap(), None);
    assert_eq!(route("echo hi", "/w", Os::Unix, &host).unwrap(), None);
    assert_eq!(route("./missing.py", "/w", Os::Unix, &host).unwrap(), None);
}

#[test]
fn a_windows_line_that_starts_with_a_py_runs_it_with_python_not_the_association() {
    let host = windows().file("C:\\Windows\\py.exe", false, b"MZ");
    let (line, how) = route(".\\tools\\gen.py --out x", "C:\\w", Os::Windows, &host).unwrap().unwrap();
    assert_eq!(line, "& 'C:\\Windows\\py.exe' .\\tools\\gen.py --out x");
    assert!(how.contains("associated app"), "{how}");
    let (line, _) = route("python -m pip list", "C:\\w", Os::Windows, &host).unwrap().unwrap();
    assert_eq!(line, "& 'C:\\Windows\\py.exe' -m pip list");
    assert_eq!(route("Get-ChildItem", "C:\\w", Os::Windows, &host).unwrap(), None);
}

#[test]
fn a_windows_python_line_with_no_python_fails_before_it_runs() {
    let error = route("python app.py", "C:\\w", Os::Windows, &windows()).unwrap_err();
    assert_eq!(error.code, "interpreter_not_found");
    let real = windows().file("C:\\Python313\\python.exe", false, b"MZ").var(
        "PATH",
        "C:\\Python313;C:\\Users\\h\\AppData\\Local\\Microsoft\\WindowsApps",
    );
    assert_eq!(route("python app.py", "C:\\w", Os::Windows, &real).unwrap(), None);
}
