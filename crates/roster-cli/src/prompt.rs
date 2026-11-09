use anstream::{eprintln, println};
use anstyle::{AnsiColor, Style};
use roster_core::ops::{Plan, StepKind};
use std::io::{BufRead, IsTerminal, Write};

pub fn print_plan(plan: &Plan) {
    let bold = Style::new().bold();
    println!("{bold}{}{bold:#}", plan.summary);
    for s in &plan.steps {
        let (mark, color) = match s.kind {
            StepKind::Add => ("+", AnsiColor::Green),
            StepKind::Change => ("~", AnsiColor::Yellow),
            StepKind::Remove => ("-", AnsiColor::Red),
            StepKind::Note => (" ", AnsiColor::White),
        };
        let st = Style::new().fg_color(Some(color.into()));
        println!("  {st}{mark}{st:#} {}", s.text);
    }
}

/// Exit code 2 when no answer is possible.
pub enum Confirm {
    Yes,
    No,
    CannotAsk,
}

pub fn confirm(yes: bool) -> Confirm {
    if yes {
        return Confirm::Yes;
    }
    if !std::io::stdin().is_terminal() {
        eprintln!("Add --yes to run without a prompt.");
        return Confirm::CannotAsk;
    }
    print!("Continue? [Y/n] ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    match line.trim().to_ascii_lowercase().as_str() {
        "" | "y" | "yes" => Confirm::Yes,
        _ => Confirm::No,
    }
}
