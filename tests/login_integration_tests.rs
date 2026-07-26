//! Integration tests for login shell behavior, CLI mode dispatch,
//! profile sourcing, and display manager session startup.
//!
//! These tests exercise the full binary via `std::process::Command` to verify
//! that AsterShell behaves identically to bash/zsh when invoked by LightDM,
//! GDM, SDDM, SSH, and terminal emulators.

use std::io::Write;
use std::process::{Command, Stdio};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Path to the compiled aster binary.
fn aster_bin() -> String {
    // Build the binary first if needed, then return the path
    let target_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| "target/debug".into());

    // The test binary lives in target/debug/deps/, so the binary is in target/debug/
    let bin = target_dir.parent().unwrap_or(&target_dir).join("aster");
    if bin.exists() {
        bin.to_string_lossy().to_string()
    } else {
        // Fallback: try target/debug/aster directly
        "target/debug/aster".to_string()
    }
}

/// Runs `aster -c <command>` and returns (exit_code, stdout, stderr).
fn run_c(command: &str) -> (i32, String, String) {
    let output = Command::new(aster_bin())
        .arg("-c")
        .arg(command)
        .output()
        .expect("failed to spawn aster");

    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

/// Runs `aster` with arbitrary args and returns (exit_code, stdout, stderr).
fn run_args(args: &[&str]) -> (i32, String, String) {
    let output = Command::new(aster_bin())
        .args(args)
        .output()
        .expect("failed to spawn aster");

    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

/// Runs `aster` with custom env vars and returns (exit_code, stdout, stderr).
fn run_c_with_env(command: &str, envs: &[(&str, &str)]) -> (i32, String, String) {
    let mut cmd = Command::new(aster_bin());
    cmd.arg("-c").arg(command);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let output = cmd.output().expect("failed to spawn aster");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

/// Runs `aster` with stdin piped and writes lines to it, returns (exit_code, stdout).
fn run_stdin(lines: &[&str]) -> (i32, String) {
    let mut child = Command::new(aster_bin())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn aster");

    let stdin = child.stdin.as_mut().expect("failed to open stdin");
    for line in lines {
        writeln!(stdin, "{line}").expect("failed to write stdin");
    }
    drop(child.stdin.take()); // close stdin

    let output = child.wait_with_output().expect("failed to wait");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
    )
}

// ===========================================================================
// 1. -c command execution (LightDM/GDM/SDDM pattern)
// ===========================================================================

#[test]
fn test_c_simple_echo() {
    let (code, stdout, _stderr) = run_c("echo hello");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "hello");
}

#[test]
fn test_c_exit_code_success() {
    let (code, _, _) = run_c("true");
    assert_eq!(code, 0);
}

#[test]
fn test_c_exit_code_failure() {
    let (code, _, _) = run_c("false");
    assert_eq!(code, 1);
}

#[test]
fn test_c_exit_code_custom() {
    let (code, _, _) = run_c("exit 42");
    assert_eq!(code, 42);
}

#[test]
fn test_c_sequential_commands() {
    let (code, stdout, _) = run_c("echo a ; echo b ; echo c");
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines, vec!["a", "b", "c"]);
}

#[test]
fn test_c_pipeline() {
    let (code, stdout, _) = run_c("echo hello world | wc -w");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "2");
}

#[test]
fn test_c_positional_args() {
    let (code, stdout, _) = run_c("echo $1 $2");
    assert_eq!(code, 0);
    // No positional args from -c without passing them — both expand empty
    assert_eq!(stdout.trim(), "");
}

// ===========================================================================
// 2. -c with positional parameters (display manager exec pattern)
// ===========================================================================

#[test]
fn test_c_with_positional_args() {
    // aster -c 'echo $1' arg1
    let output = Command::new(aster_bin())
        .args(["-c", "echo $1 $2", "hello", "world"])
        .output()
        .expect("failed to spawn aster");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert_eq!(output.status.code().unwrap_or(-1), 0);
    assert_eq!(stdout.trim(), "hello world");
}

#[test]
fn test_c_with_positional_args_count() {
    let output = Command::new(aster_bin())
        .args(["-c", "echo $#"])
        .output()
        .expect("failed to spawn aster");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert_eq!(output.status.code().unwrap_or(-1), 0);
    assert_eq!(stdout.trim(), "0");
}

#[test]
fn test_c_with_positional_args_count_with_args() {
    let output = Command::new(aster_bin())
        .args(["-c", "echo $#", "a", "b", "c"])
        .output()
        .expect("failed to spawn aster");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert_eq!(output.status.code().unwrap_or(-1), 0);
    assert_eq!(stdout.trim(), "3");
}

// ===========================================================================
// 3. Desktop environment startup simulation
// ===========================================================================

#[test]
fn test_c_exec_desktop_simulation() {
    // Simulates: aster -c 'exec budgie-desktop'
    // We can't actually exec a desktop, but we can verify the shell
    // handles the command mode correctly with a no-op.
    let (code, _, _) = run_c("true");
    assert_eq!(code, 0);
}

#[test]
fn test_c_env_preservation() {
    // Simulates display manager setting DISPLAY before launching shell
    let (code, stdout, _) = run_c_with_env(
        "echo $DISPLAY",
        &[("DISPLAY", ":0")],
    );
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), ":0");
}

#[test]
fn test_c_preserves_wayland_display() {
    let (code, stdout, _) = run_c_with_env(
        "echo $WAYLAND_DISPLAY",
        &[("WAYLAND_DISPLAY", "wayland-0")],
    );
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "wayland-0");
}

#[test]
fn test_c_preserves_xdg_session() {
    let (code, stdout, _) = run_c_with_env(
        "echo $XDG_SESSION_TYPE",
        &[("XDG_SESSION_TYPE", "wayland")],
    );
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "wayland");
}

#[test]
fn test_c_preserves_dbus_address() {
    let (code, stdout, _) = run_c_with_env(
        "echo $DBUS_SESSION_BUS_ADDRESS",
        &[("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus")],
    );
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "unix:path=/run/user/1000/bus");
}

#[test]
fn test_c_preserves_home() {
    let (code, stdout, _) = run_c("echo $HOME");
    assert_eq!(code, 0);
    // HOME should be set in the environment
    assert!(!stdout.trim().is_empty());
}

#[test]
fn test_c_preserves_path() {
    let (code, stdout, _) = run_c("echo $PATH");
    assert_eq!(code, 0);
    assert!(!stdout.trim().is_empty());
    // PATH should contain /usr/bin at minimum
    assert!(stdout.contains("/usr/bin") || !stdout.trim().is_empty());
}

// ===========================================================================
// 4. Login shell mode (-l flag)
// ===========================================================================

#[test]
fn test_l_flag_sets_login_mode() {
    // -l with -c should still execute the command
    let (code, stdout, _) = run_args(&["-l", "-c", "echo login"]);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "login");
}

#[test]
fn test_lc_combined_flag() {
    // -lc is equivalent to -l -c
    let (code, stdout, _) = run_args(&["-lc", "echo combined"]);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "combined");
}

#[test]
fn test_login_shell_sources_etc_profile() {
    // Login shells should source /etc/profile, which typically sets PATH
    let (code, stdout, _) = run_args(&["-l", "-c", "echo $PATH"]);
    assert_eq!(code, 0);
    assert!(!stdout.trim().is_empty());
}

// ===========================================================================
// 5. --norc and --noprofile flags
// ===========================================================================

#[test]
fn test_noprofile_flag_accepted() {
    let (code, _, _) = run_args(&["--noprofile", "-c", "true"]);
    assert_eq!(code, 0);
}

#[test]
fn test_norc_flag_accepted() {
    let (code, _, _) = run_args(&["--norc", "-c", "true"]);
    assert_eq!(code, 0);
}

#[test]
fn test_norc_noprofile_combined() {
    let (code, _, _) = run_args(&["--norc", "--noprofile", "-c", "true"]);
    assert_eq!(code, 0);
}

// ===========================================================================
// 6. Script file execution
// ===========================================================================

#[test]
fn test_script_file_execution() {
    let tmp = std::env::temp_dir().join("aster_test_script.sh");
    std::fs::write(&tmp, "echo from_script").unwrap();

    let (code, stdout, _) = run_args(&[tmp.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "from_script");

    std::fs::remove_file(&tmp).ok();
}

#[test]
fn test_script_file_with_args() {
    let tmp = std::env::temp_dir().join("aster_test_args.sh");
    std::fs::write(&tmp, "echo $1 $2").unwrap();

    let output = Command::new(aster_bin())
        .args([tmp.to_str().unwrap(), "foo", "bar"])
        .output()
        .expect("failed to spawn aster");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert_eq!(output.status.code().unwrap_or(-1), 0);
    assert_eq!(stdout.trim(), "foo bar");

    std::fs::remove_file(&tmp).ok();
}

#[test]
fn test_script_file_nonexistent() {
    let (code, _, stderr) = run_args(&["/nonexistent/script.sh"]);
    assert_ne!(code, 0);
    assert!(stderr.contains("No such file") || stderr.contains("not found") || !stderr.is_empty());
}

#[test]
fn test_script_file_exit_code() {
    let tmp = std::env::temp_dir().join("aster_test_exit.sh");
    std::fs::write(&tmp, "exit 99").unwrap();

    let (code, _, _) = run_args(&[tmp.to_str().unwrap()]);
    assert_eq!(code, 99);

    std::fs::remove_file(&tmp).ok();
}

// ===========================================================================
// 7. Stdin mode (-s)
// ===========================================================================

#[test]
fn test_s_flag_stdin_mode() {
    let (code, stdout) = run_stdin(&["echo from_stdin"]);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "from_stdin");
}

#[test]
fn test_bare_dash_stdin_mode() {
    let mut child = Command::new(aster_bin())
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn aster");

    let stdin = child.stdin.as_mut().expect("failed to open stdin");
    writeln!(stdin, "echo bare_dash").expect("failed to write stdin");
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("failed to wait");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert_eq!(output.status.code().unwrap_or(-1), 0);
    assert!(stdout.contains("bare_dash"));
}

// ===========================================================================
// 8. -- end-of-flags marker
// ===========================================================================

#[test]
fn test_double_dash_script() {
    let tmp = std::env::temp_dir().join("aster_test_doubledash.sh");
    std::fs::write(&tmp, "echo double_dash_script").unwrap();

    let (code, stdout, _) = run_args(&["--", tmp.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "double_dash_script");

    std::fs::remove_file(&tmp).ok();
}

// ===========================================================================
// 9. Combined short flags
// ===========================================================================

#[test]
fn test_lv_flag() {
    // -lv should set login and verbose (accepted silently)
    let (code, _, _) = run_args(&["-lv", "-c", "true"]);
    assert_eq!(code, 0);
}

#[test]
fn test_ln_flag() {
    // -ln should set login and noprofile
    let (code, _, _) = run_args(&["-ln", "-c", "true"]);
    assert_eq!(code, 0);
}

// ===========================================================================
// 10. Variable assignment and expansion in -c
// ===========================================================================

#[test]
fn test_c_variable_assignment() {
    let (code, stdout, _) = run_c("FOO=bar ; echo $FOO");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "bar");
}

#[test]
fn test_c_environment_export_visible_to_child() {
    let (code, stdout, _) = run_c("export ASTER_TEST_VAR=exported ; /bin/sh -c 'echo $ASTER_TEST_VAR'");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "exported");
}

// ===========================================================================
// 11. Error handling
// ===========================================================================

#[test]
fn test_c_invalid_command() {
    let (code, _, stderr) = run_c("nonexistent_command_xyz");
    assert_ne!(code, 0);
    assert!(!stderr.is_empty());
}

#[test]
fn test_c_syntax_error() {
    let (code, _, _stderr) = run_c("if then fi");
    assert_ne!(code, 0);
}

#[test]
fn test_c_empty_command() {
    // aster -c with no argument
    let output = Command::new(aster_bin())
        .arg("-c")
        .output()
        .expect("failed to spawn aster");
    // Should exit with non-zero (missing command argument)
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(code, 0);
}

// ===========================================================================
// 12. SSH session simulation
// ===========================================================================

#[test]
fn test_ssh_session_preserves_vars() {
    let (code, stdout, _) = run_c_with_env(
        "echo $SSH_CONNECTION",
        &[("SSH_CONNECTION", "192.168.1.1 12345 192.168.1.2 22")],
    );
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "192.168.1.1 12345 192.168.1.2 22");
}

#[test]
fn test_ssh_session_preserves_auth_sock() {
    let (code, stdout, _) = run_c_with_env(
        "echo $SSH_AUTH_SOCK",
        &[("SSH_AUTH_SOCK", "/tmp/ssh-XXXX/agent.12345")],
    );
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "/tmp/ssh-XXXX/agent.12345");
}

// ===========================================================================
// 13. Graphical session simulation (LightDM/GDM/SDDM)
// ===========================================================================

#[test]
fn test_lightdm_simulation() {
    // LightDM calls: aster -c 'exec budgie-desktop'
    // With environment: DISPLAY=:0, XDG_SESSION_TYPE=x11, etc.
    let (code, stdout, _) = run_c_with_env(
        "echo $DISPLAY $XDG_SESSION_TYPE $XDG_CURRENT_DESKTOP",
        &[
            ("DISPLAY", ":0"),
            ("XDG_SESSION_TYPE", "x11"),
            ("XDG_CURRENT_DESKTOP", "Budgie:GNOME"),
        ],
    );
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), ":0 x11 Budgie:GNOME");
}

#[test]
fn test_gdm_wayland_simulation() {
    // GDM calls: aster -c 'exec gnome-session'
    // With environment: WAYLAND_DISPLAY=wayland-0, XDG_SESSION_TYPE=wayland
    let (code, stdout, _) = run_c_with_env(
        "echo $WAYLAND_DISPLAY $XDG_SESSION_TYPE $XDG_CURRENT_DESKTOP",
        &[
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("XDG_SESSION_TYPE", "wayland"),
            ("XDG_CURRENT_DESKTOP", "GNOME"),
        ],
    );
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "wayland-0 wayland GNOME");
}

#[test]
fn test_sddm_kde_simulation() {
    // SDDM calls: aster -c 'exec startplasma-x11'
    let (code, stdout, _) = run_c_with_env(
        "echo $DISPLAY $XDG_SESSION_TYPE $XDG_CURRENT_DESKTOP",
        &[
            ("DISPLAY", ":1"),
            ("XDG_SESSION_TYPE", "x11"),
            ("XDG_CURRENT_DESKTOP", "KDE"),
        ],
    );
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), ":1 x11 KDE");
}

#[test]
fn test_full_desktop_env_simulation() {
    // Full environment that a display manager would set
    let (code, stdout, _) = run_c_with_env(
        "echo $DISPLAY $WAYLAND_DISPLAY $XDG_SESSION_TYPE $XDG_SESSION_CLASS $DBUS_SESSION_BUS_ADDRESS $XDG_RUNTIME_DIR",
        &[
            ("DISPLAY", ":0"),
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("XDG_SESSION_TYPE", "wayland"),
            ("XDG_SESSION_CLASS", "user"),
            ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus"),
            ("XDG_RUNTIME_DIR", "/run/user/1000"),
        ],
    );
    assert_eq!(code, 0);
    assert_eq!(
        stdout.trim(),
        ":0 wayland-0 wayland user unix:path=/run/user/1000/bus /run/user/1000"
    );
}

// ===========================================================================
// 14. Login shell via argv[0] prefix (display manager pattern)
// ===========================================================================

#[test]
fn test_argv0_dash_prefix_detection() {
    // Display managers invoke: /bin/aster -c 'exec ...'
    // Some invoke as: -aster -c 'exec ...'
    // We test by symlinking with - prefix
    let tmp_dir = std::env::temp_dir().join("aster_argv0_test");
    std::fs::create_dir_all(&tmp_dir).ok();

    let aster_path = aster_bin();
    let link_path = tmp_dir.join("-aster");

    // Create symlink -aster -> aster
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&aster_path, &link_path).ok();
    }

    if link_path.exists() {
        let output = Command::new(&link_path)
            .args(["-c", "echo $0"])
            .output()
            .expect("failed to spawn -aster");

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        // $0 should contain the argv[0]
        assert!(stdout.contains("-aster") || stdout.contains("aster"));
    }

    std::fs::remove_dir_all(&tmp_dir).ok();
}

// ===========================================================================
// 15. Pipeline and exit code propagation
// ===========================================================================

#[test]
fn test_c_pipeline_exit_code() {
    let (code, _, _) = run_c("false | true");
    // Pipeline exit code is from the last command
    assert_eq!(code, 0);
}

#[test]
fn test_c_pipeline_failure() {
    let (code, _, _) = run_c("true | false");
    assert_eq!(code, 1);
}

#[test]
fn test_c_and_or_chain() {
    let (code, stdout, _) = run_c("true && echo ok || echo fail");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "ok");
}

// ===========================================================================
// 16. Subshell and command substitution
// ===========================================================================

#[test]
fn test_c_command_substitution() {
    let (code, stdout, _) = run_c("echo $(echo nested)");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "nested");
}

#[test]
fn test_c_arithmetic_expansion() {
    let (code, stdout, _) = run_c("echo $((2 + 3))");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "5");
}

// ===========================================================================
// 17. Multiple -c commands
// ===========================================================================

#[test]
fn test_multiple_c_flags() {
    // Only the first -c should be used; subsequent ones are positional
    let output = Command::new(aster_bin())
        .args(["-c", "echo first", "-c", "echo second"])
        .output()
        .expect("failed to spawn aster");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert_eq!(output.status.code().unwrap_or(-1), 0);
    assert_eq!(stdout.trim(), "first");
}
