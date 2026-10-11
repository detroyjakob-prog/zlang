//! The Z programming language toolchain.
//!
//! Usage:
//!     z run file.z     run a program
//!     z check file.z   check syntax without running
//!     z build file.z   build a standalone executable
//!     z repl           interactive prompt
//!     z --version      print the version

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use zlang::error::{ZError, ZResult};
use zlang::compiler;
use zlang::Vm;

const VERSION: &str = "0.2.0";

/// Terminal styling helpers — colors and boxed panels drawn with plain ANSI
/// escape sequences and no external dependencies.
///
/// Styling is applied only when the relevant stream is an interactive terminal
/// and `NO_COLOR` is unset, so redirected or piped output stays clean and
/// machine-readable. Only the CLI's own chrome is styled here; a Z program's
/// output (via `printer`, `z run`, or a `z build` artifact) is left untouched.
mod ui {
    use std::io::IsTerminal;

    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
    pub const DIM: &str = "\x1b[2m";
    pub const RED: &str = "\x1b[31m";
    pub const GREEN: &str = "\x1b[32m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const CYAN: &str = "\x1b[36m";
    pub const GRAY: &str = "\x1b[90m";
    /// Bold cyan, used to highlight command names.
    pub const ACCENT: &str = "\x1b[1;36m";

    fn no_color() -> bool {
        std::env::var_os("NO_COLOR").is_some()
    }

    pub fn color_stdout() -> bool {
        std::io::stdout().is_terminal() && !no_color()
    }

    pub fn color_stderr() -> bool {
        std::io::stderr().is_terminal() && !no_color()
    }

    /// Wrap `text` in an SGR `code` when stdout coloring is active.
    fn wrap(code: &str, text: &str) -> String {
        if color_stdout() {
            format!("{code}{text}{RESET}")
        } else {
            text.to_string()
        }
    }

    /// Bold-cyan `text`, for command names and headings.
    pub fn accent(text: &str) -> String {
        wrap(ACCENT, text)
    }

    /// Dim `text`, for secondary detail.
    pub fn dim(text: &str) -> String {
        wrap(DIM, text)
    }

    /// Dim `text`, for argument placeholders.
    pub fn arg(text: &str) -> String {
        wrap(DIM, text)
    }

    /// Print a dim note to stdout.
    pub fn note(msg: &str) {
        if color_stdout() {
            println!("{DIM}{msg}{RESET}");
        } else {
            println!("{msg}");
        }
    }

    /// Print a green success line to stdout.
    pub fn success(msg: &str) {
        if color_stdout() {
            println!("{GREEN}{BOLD}✔{RESET} {msg}");
        } else {
            println!("OK  {msg}");
        }
    }

    /// Print a yellow warning to stderr.
    pub fn warn(msg: &str) {
        if color_stderr() {
            eprintln!("{YELLOW}{BOLD}⚠{RESET} {msg}");
        } else {
            eprintln!("warning: {msg}");
        }
    }

    /// Print a red error to stderr.
    pub fn error(msg: &str) {
        if color_stderr() {
            eprintln!("{RED}{BOLD}✖{RESET} {msg}");
        } else {
            eprintln!("error: {msg}");
        }
    }

    /// Visible width of `s`, ignoring ANSI SGR escape sequences so alignment
    /// stays correct even for pre-colored body lines.
    fn visible_len(s: &str) -> usize {
        let mut len = 0usize;
        let mut in_esc = false;
        for ch in s.chars() {
            if in_esc {
                if ch == 'm' {
                    in_esc = false;
                }
            } else if ch == '\x1b' {
                in_esc = true;
            } else {
                len += 1;
            }
        }
        len
    }

    /// Draw a rounded panel with a titled top border around `body` lines.
    pub fn panel(title: &str, body: &[String]) {
        let enabled = color_stdout();
        let (border, accent, reset) = if enabled {
            (CYAN, ACCENT, RESET)
        } else {
            ("", "", "")
        };

        let title_w = visible_len(title);
        let mut w = title_w + 2;
        for line in body {
            w = w.max(visible_len(line));
        }
        let inner = w + 2; // columns between the two corners

        let dashes = inner.saturating_sub(3 + title_w);
        println!("{border}╭─ {accent}{title}{border} {dash}╮{reset}", dash = "─".repeat(dashes));

        for line in body {
            let pad = " ".repeat(w.saturating_sub(visible_len(line)));
            println!("{border}│{reset} {line}{pad} {border}│{reset}");
        }

        println!("{border}╰{dash}╯{reset}", dash = "─".repeat(inner));
    }

    /// The REPL prompt, e.g. a green `z` with a dim chevron.
    pub fn prompt() -> String {
        if color_stdout() {
            format!("{GREEN}{BOLD}z{RESET}{GRAY}❯{RESET} ")
        } else {
            "z> ".to_string()
        }
    }
}

fn main() {
    // A binary produced by `z build` carries its program appended to this same
    // executable. When present, run it directly and ignore CLI arguments so the
    // artifact behaves like an ordinary compiled program.
    if let Some(source) = zlang::native::embedded_source() {
        std::process::exit(run_embedded(source));
    }

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        print_usage();
        return;
    }
    match run(args) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            ui::error(&e.report());
            std::process::exit(1);
        }
    }
}

fn run(args: Vec<String>) -> ZResult<i32> {
    let command = args[0].as_str();
    match command {
        "--version" | "-v" | "version" => {
            print_banner();
            Ok(0)
        }
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(0)
        }
        "run" => run_file(&args).map(|c| c),
        "check" => check_file(&args),
        "build" => build_file(&args),
        "repl" => repl(),
        other => {
            ui::error(&format!("unknown command '{}'", other));
            println!();
            print_usage();
            Ok(1)
        }
    }
}

/// The Z wordmark and version, shown for `--version` and at the top of help.
fn print_banner() {
    if ui::color_stdout() {
        // A compact block-letter "Z" wordmark.
        println!(
            r"█▀▀▀▀▀██
    ▄█▀
  ▄█▀  
▄█▀    ▄
▀▀▀▀▀▀▀▀{b}{c}{reset}",
            b = ui::BOLD,
            c = ui::CYAN,
            reset = ui::RESET,
        );
        println!(
            "{bold}Z Programming Language{reset}  {dim}v{VERSION}{reset}",
            bold = ui::BOLD,
            reset = ui::RESET,
            dim = ui::DIM,
        );
    } else {
        println!("Z Programming Language  v{VERSION}");
    }
}

fn print_usage() {
    print_banner();
    println!();

    // Pad the plain text first, then color it, so column alignment is
    // unaffected by the (invisible) escape sequences.
    let cmd = |name: &str| ui::accent(&format!("{:<9}", name));
    let file = ui::arg(&format!("{:<12}", "file.z"));
    let none = ui::arg(&format!("{:<12}", ""));

    ui::panel("Usage", &[
        format!("  {} {} Run a program", cmd("run"), file),
        format!("  {} {} Check syntax without running", cmd("check"), file),
        format!("  {} {} Build a standalone executable", cmd("build"), file),
        format!("  {} {} Start the interactive prompt", cmd("repl"), none),
        format!("  {} {} Print the version", cmd("--version"), none),
    ]);

    println!();
    ui::note("Build accepts -o <path> to choose the output name;");
    ui::note("otherwise the executable takes the source name without .z.");
}

/// Build a VM with the standard library installed and imports rooted at `dir`.
fn make_vm(dir: &Path) -> Vm {
    let mut vm = Vm::new().with_base_dir(dir.to_path_buf());
    vm.install_builtins();
    vm
}

fn run_file(args: &[String]) -> ZResult<i32> {
    let path = args.get(1).ok_or_else(|| {
        ZError::Runtime("missing file: usage 'z run file.z'".to_string())
    })?;
    let source = read_source(Path::new(path))?;
    let base = base_dir(Path::new(path));
    let module = compile(&source, Some(path))?;

    let mut vm = make_vm(&base);
    vm.run(&module)?;
    Ok(0)
}

fn check_file(args: &[String]) -> ZResult<i32> {
    let path = args.get(1).ok_or_else(|| {
        ZError::Runtime("missing file: usage 'z check file.z'".to_string())
    })?;
    let source = read_source(Path::new(path))?;
    compile(&source, Some(path))?;
    ui::success(&format!("{}: syntax and analysis passed", path));
    Ok(0)
}

fn read_source(path: &Path) -> ZResult<String> {
    std::fs::read_to_string(path).map_err(|e| {
        ZError::Runtime(format!("cannot read '{}': {}", path.display(), e))
    })
}

/// Compile `file.z` into a standalone native executable.
///
/// The source is fully compiled and analyzed first, so a build fails fast on
/// any syntax or semantic error rather than shipping a broken artifact. The
/// resulting binary embeds the program and runs it through the same VM as
/// `z run`, needing neither the `.z` file nor a separate Z install.
fn build_file(args: &[String]) -> ZResult<i32> {
    let path = args.get(1).ok_or_else(|| {
        ZError::Runtime("missing file: usage 'z build file.z [-o output]'".to_string())
    })?;

    let source = read_source(Path::new(path))?;
    // Analyze up front so errors are reported before we write any output.
    compile(&source, Some(path))?;

    let output = match args.windows(2).find(|pair| pair[0] == "-o") {
        Some(pair) if !pair[1].starts_with('-') => PathBuf::from(&pair[1]),
        Some(_) => {
            return Err(ZError::Runtime("missing output path after -o".to_string()));
        }
        None => {
            let mut output = PathBuf::from(path);
            output.set_extension("");
            output
        }
    };

    zlang::native::build_executable(&source, &output)?;

    let size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
    ui::success(&format!(
        "Built {}  {}",
        ui::accent(&output.display().to_string()),
        ui::dim(&format!("({})", human_size(size))),
    ));
    Ok(0)
}

/// Format a byte count as a short human-readable size, e.g. `1.5 MiB`.
fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.1} {}", value, UNITS[unit])
    }
}

/// Run the program embedded inside a `z build` artifact. This is the entry
/// point taken when the executable itself carries a Z program.
fn run_embedded(source: String) -> i32 {
    let base = base_dir(Path::new(&std::env::args().next().unwrap_or_default()));
    let module = match compile(&source, None) {
        Ok(m) => m,
        Err(e) => {
            ui::error(&e.report());
            return 1;
        }
    };
    let mut vm = make_vm(&base);
    match vm.run(&module) {
        Ok(()) => 0,
        Err(e) => {
            ui::error(&e.report());
            1
        }
    }
}

/// Compile and analyze a module, reporting errors with the file name.
fn compile(source: &str, path: Option<&str>) -> ZResult<compiler::Module> {
    let module = compiler::compile(source, path).map_err(|e| match (&e, path) {
        (ZError::Lexer(m), Some(p)) => ZError::Lexer(format!("{}: {}", p, m)),
        (ZError::Parse(m), Some(p)) => ZError::Parse(format!("{}: {}", p, m)),
        (ZError::Compile(m), Some(p)) => ZError::Compile(format!("{}: {}", p, m)),
        (ZError::ImportError(m), Some(p)) => ZError::ImportError(format!("{}: {}", p, m)),
        (ZError::Runtime(m), Some(p)) => ZError::Runtime(format!("{}: {}", p, m)),
        (ZError::Raised(r), Some(p)) => ZError::raised(r.class_name.clone(), format!("{}: {}", p, r.message)),
        (e, _) => e.clone(),
    })?;
    for warning in &module.warnings {
        ui::warn(warning);
    }
    Ok(module)
}

fn base_dir(path: &Path) -> PathBuf {
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

fn repl() -> ZResult<i32> {
    ui::panel(
        &format!("Z REPL  v{}", VERSION),
        &[ui::dim("Type 'exit' or press Ctrl-D to leave.")],
    );
    let mut vm = make_vm(Path::new("."));
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    loop {
        print!("{}", ui::prompt());
        let _ = stdout.flush();
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                println!();
                break;
            }
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed == "exit" || trimmed == "quit" {
                    break;
                }
                match repl_eval(&mut vm, trimmed) {
                    Ok(()) => {}
                    Err(e) => ui::error(&e.report()),
                }
            }
            Err(_) => break,
        }
    }
    Ok(0)
}

fn repl_eval(vm: &mut Vm, line: &str) -> ZResult<()> {
    let module = compile(line, None)?;
    match vm.exec_block(&module.statements())? {
        Some(v) => {
            let text = vm.to_string_value(&v);
            if !text.is_empty() && text != "None" {
                println!("{}", text);
            }
            Ok(())
        }
        None => Ok(()),
    }
}
