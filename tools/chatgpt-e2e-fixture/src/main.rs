use std::env;
use std::process::Command;
use std::thread;
use std::time::Duration;

fn main() {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments
        .iter()
        .any(|argument| argument == "--type=renderer")
    {
        wait_for_termination();
        return;
    }

    if arguments.iter().any(|argument| argument == "--lang=zh-CN") {
        let executable = match env::current_exe() {
            Ok(path) => path,
            Err(_) => std::process::exit(1),
        };
        if Command::new(executable)
            .args(["--type=renderer", "--lang=zh-CN"])
            .spawn()
            .is_err()
        {
            std::process::exit(1);
        }
    }

    wait_for_termination();
}

fn wait_for_termination() {
    loop {
        thread::sleep(Duration::from_secs(60));
    }
}
