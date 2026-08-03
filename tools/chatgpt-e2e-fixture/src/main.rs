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

    if let Some(locale) = arguments
        .iter()
        .find_map(|argument| argument.strip_prefix("--lang="))
    {
        let executable = match env::current_exe() {
            Ok(path) => path,
            Err(_) => std::process::exit(1),
        };
        if Command::new(executable)
            .arg("--type=renderer")
            .arg(format!("--lang={locale}"))
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
