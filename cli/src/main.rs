//! cadmus-cli：阅读引擎开发工具（DESIGN-ENGINE §2.1）。
//!
//! 约定：stdout 只放结果，stderr 放诊断；退出码 0 成功 / 1 输入错误 / 2 降级成功。

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use cadmus_core::{Error, Result};
use cadmus_epub::{TxtOptions, parse_txt_bytes};
use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "cadmus-cli", version, about = "cadmus 阅读引擎开发工具（M1）")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// PDF 重排：分栏检测 → 单列流式
    Reflow(ReflowArgs),
    /// EPUB 解析
    Epub(EpubArgs),
    /// TXT 解析：编码探测 + 章节切分
    Txt(TxtArgs),
    /// 解析计时（×n 中位数，CR-7 口径）
    Bench(BenchArgs),
}

#[derive(Args)]
struct ReflowArgs {
    file: PathBuf,
    /// 目标阅读宽度（CSS px；M1 仅记录，不参与重排）
    #[arg(long, default_value = "600x800")]
    viewport: String,
    /// 页范围，如 1-20
    #[arg(long)]
    pages: Option<String>,
    /// 输出格式
    #[arg(long, value_enum, default_value_t = ReflowDump::Text)]
    dump: ReflowDump,
    /// 位图导出目录（原子块/降级页）
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum)]
enum ReflowDump {
    Text,
    Json,
}

#[derive(Args)]
struct EpubArgs {
    file: PathBuf,
    /// 输出格式
    #[arg(long, value_enum, default_value_t = EpubDump::Text)]
    dump: EpubDump,
    /// 图片导出目录
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum)]
enum EpubDump {
    Text,
    Blocks,
}

#[derive(Args)]
struct TxtArgs {
    file: PathBuf,
    /// 手动指定编码（覆盖探测）：utf-8 / gb18030 / big5 / utf-16le ...
    #[arg(long)]
    encoding: Option<String>,
    /// 输出格式
    #[arg(long, value_enum, default_value_t = TxtDump::Text)]
    dump: TxtDump,
}

#[derive(Clone, Copy, ValueEnum)]
enum TxtDump {
    /// 阅读文本（每块一行）
    Text,
    /// 章节列表
    Chapters,
}

#[derive(Args)]
struct BenchArgs {
    file: PathBuf,
    /// 重复次数
    #[arg(long, default_value_t = 5)]
    repeat: usize,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("错误：{error}");
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::Reflow(args) => Err(Error::Unsupported(format!(
            "reflow 尚未接通（{}，viewport={}，pages={:?}）",
            args.file.display(),
            args.viewport,
            args.pages
        ))),
        Command::Epub(args) => Err(Error::Unsupported(format!(
            "epub 尚未接通（{}）",
            args.file.display()
        ))),
        Command::Txt(args) => run_txt(&args),
        Command::Bench(args) => run_bench(&args),
    }
}

fn run_txt(args: &TxtArgs) -> Result<ExitCode> {
    let bytes = std::fs::read(&args.file)?;
    let options = TxtOptions {
        encoding: args.encoding.clone(),
        ..TxtOptions::default()
    };
    let (document, report) = parse_txt_bytes(&bytes, &options)?;
    eprintln!(
        "编码 {}；{} 字节；{} 个块",
        report.encoding,
        bytes.len(),
        document.blocks.len()
    );
    if report.had_errors {
        eprintln!("警告：输入包含非法字节，已替换为 U+FFFD");
    }

    match args.dump {
        TxtDump::Text => {
            for block in &document.blocks {
                if let Some(text) = block.as_text() {
                    println!("{text}");
                }
            }
        }
        TxtDump::Chapters => {
            let mut index = 0usize;
            for block in &document.blocks {
                if let cadmus_core::BlockKind::Heading { level } = block.kind {
                    index += 1;
                    println!("{index:>4}  L{level}  {}", block.as_text().unwrap_or(""));
                }
            }
        }
    }

    Ok(if report.had_errors {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    })
}

fn run_bench(args: &BenchArgs) -> Result<ExitCode> {
    // 目前仅 TXT；reflow/epub 接通后按扩展名分发。
    let bytes = std::fs::read(&args.file)?;
    let mut timings = Vec::new();
    for _ in 0..args.repeat.max(1) {
        let start = Instant::now();
        let _ = parse_txt_bytes(&bytes, &TxtOptions::default())?;
        timings.push(start.elapsed());
    }
    timings.sort();
    let median = timings[timings.len() / 2];
    println!("{} 次：中位 {:?}", timings.len(), median);
    Ok(ExitCode::SUCCESS)
}
