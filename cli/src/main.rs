//! cadmus-cli：阅读引擎开发工具（DESIGN-ENGINE §2.1）。
//!
//! 约定：stdout 只放结果，stderr 放诊断；退出码 0 成功 / 1 输入错误 / 2 降级成功。

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use cadmus_core::{Block, BlockKind, Cancel, Document, Error, Payload, Result};
use cadmus_epub::{EpubOptions, TxtOptions, parse_epub, parse_txt_bytes};
use cadmus_preview::{PreviewMeta, render};
use cadmus_reflow::{ReflowOptions, reflow};
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
    /// HTML 预览（含公式/图位图，浏览器直接打开）
    Html,
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
    /// HTML 预览（含图片位图，浏览器直接打开）
    Html,
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
        Command::Reflow(args) => run_reflow(&args),
        Command::Epub(args) => run_epub(&args),
        Command::Txt(args) => run_txt(&args),
        Command::Bench(args) => run_bench(&args),
    }
}

fn run_reflow(args: &ReflowArgs) -> Result<ExitCode> {
    let _ = &args.viewport; // M1 仅记录（DESIGN-ENGINE §2.1）
    let options = ReflowOptions {
        page_range: parse_page_range(args.pages.as_deref())?,
        out_dir: args
            .out
            .clone()
            .unwrap_or_else(|| PathBuf::from("cadmus-out")),
    };
    let cancel = Cancel::new();
    let document = reflow(&args.file, &options, &cancel)?;

    let degraded = document
        .blocks
        .iter()
        .filter(|block| block.kind == BlockKind::PageBitmap)
        .count();
    eprintln!(
        "{} 个块{}",
        document.blocks.len(),
        if degraded > 0 {
            format!("，{degraded} 页整页位图降级")
        } else {
            String::new()
        }
    );

    match args.dump {
        ReflowDump::Text => {
            for block in &document.blocks {
                print_block_text(block);
            }
        }
        ReflowDump::Json => {
            println!("{}", document_json(&document));
        }
        ReflowDump::Html => {
            let title = args
                .file
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| "cadmus".to_string());
            let meta = PreviewMeta {
                title: &title,
                byline: None,
                cover: None,
                asset_src: None,
            };
            let path = render(&document, &options.out_dir, &meta)?;
            eprintln!("HTML 预览：{}", path.display());
        }
    }

    Ok(if degraded > 0 {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    })
}

fn print_block_text(block: &Block) {
    let page = block.page_range.map(|(start, _)| start + 1).unwrap_or(0);
    match (&block.kind, &block.payload) {
        (_, Payload::Text(text)) => println!("{text}"),
        (BlockKind::Figure, Payload::Bitmap(_)) => println!("[图 p.{page}]"),
        (BlockKind::Formula, Payload::Bitmap(_)) => println!("[公式 p.{page}]"),
        (BlockKind::Table, Payload::Bitmap(_)) => println!("[表格 p.{page}]"),
        (BlockKind::Code, Payload::Bitmap(_)) => println!("[代码 p.{page}]"),
        (BlockKind::PageBitmap, Payload::Bitmap(_)) => println!("[整页位图 p.{page}（降级）]"),
        (_, Payload::Bitmap(_)) => println!("[位图 p.{page}]"),
    }
}

fn document_json(document: &Document) -> serde_json::Value {
    let blocks: Vec<serde_json::Value> = document.blocks.iter().map(block_json).collect();
    serde_json::json!({
        "format_version": 1,
        "doc_hash": document.doc_hash,
        "title": document.title,
        "blocks": blocks,
    })
}

fn block_json(block: &Block) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert("id".into(), block.id.into());
    map.insert("order".into(), block.order.into());
    let kind = match block.kind {
        BlockKind::Heading { level } => {
            map.insert("level".into(), level.into());
            "heading"
        }
        BlockKind::Paragraph => "paragraph",
        BlockKind::Figure => "figure",
        BlockKind::Table => "table",
        BlockKind::Formula => "formula",
        BlockKind::Code => "code",
        BlockKind::Footnote => "footnote",
        BlockKind::PageBitmap => "page_bitmap",
    };
    map.insert("kind".into(), kind.into());
    if let Some((start, end)) = block.page_range {
        map.insert("page_range".into(), serde_json::json!([start, end]));
    }
    match &block.payload {
        Payload::Text(text) => {
            map.insert("text".into(), text.clone().into());
        }
        Payload::Bitmap(bitmap) => {
            map.insert(
                "bitmap".into(),
                serde_json::json!({
                    "path": bitmap.path,
                    "width_px": bitmap.width_px,
                    "height_px": bitmap.height_px,
                }),
            );
        }
    }
    serde_json::Value::Object(map)
}

fn run_epub(args: &EpubArgs) -> Result<ExitCode> {
    let out_dir = args
        .out
        .clone()
        .unwrap_or_else(|| PathBuf::from("cadmus-out"));
    let options = EpubOptions {
        out_dir: if matches!(args.dump, EpubDump::Html) || args.out.is_some() {
            Some(out_dir.clone())
        } else {
            None
        },
    };
    let (document, report) = parse_epub(&args.file, &options)?;
    for warning in &report.warnings {
        eprintln!("警告：{warning}");
    }
    eprintln!(
        "{} 章；{} 个块；语言 {}",
        report.chapters,
        document.blocks.len(),
        report.language.as_deref().unwrap_or("未标注")
    );
    let authors = if report.creators.is_empty() {
        None
    } else {
        Some(report.creators.join("、"))
    };
    if let Some(byline) = &authors {
        eprintln!("作者：{byline}");
    }

    match args.dump {
        EpubDump::Text => {
            for block in &document.blocks {
                print_block_text(block);
            }
        }
        EpubDump::Html => {
            let title = report
                .title
                .clone()
                .or_else(|| {
                    args.file
                        .file_stem()
                        .map(|stem| stem.to_string_lossy().into_owned())
                })
                .unwrap_or_else(|| "cadmus".to_string());
            let meta = PreviewMeta {
                title: &title,
                byline: authors.as_deref(),
                cover: report.cover_path.as_deref(),
                asset_src: None,
            };
            let path = render(&document, &out_dir, &meta)?;
            eprintln!("HTML 预览：{}", path.display());
        }
        EpubDump::Blocks => {
            for block in &document.blocks {
                match &block.payload {
                    Payload::Text(text) => println!(
                        "{:>4}  {:10}  {} 字符",
                        block.order,
                        kind_name(&block.kind),
                        text.chars().count()
                    ),
                    Payload::Bitmap(bitmap) => println!(
                        "{:>4}  {:10}  {}",
                        block.order,
                        kind_name(&block.kind),
                        bitmap.path
                    ),
                }
            }
        }
    }

    Ok(if report.warnings.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    })
}

fn kind_name(kind: &BlockKind) -> &'static str {
    match kind {
        BlockKind::Heading { .. } => "heading",
        BlockKind::Paragraph => "paragraph",
        BlockKind::Figure => "figure",
        BlockKind::Table => "table",
        BlockKind::Formula => "formula",
        BlockKind::Code => "code",
        BlockKind::Footnote => "footnote",
        BlockKind::PageBitmap => "page_bitmap",
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
                if let BlockKind::Heading { level } = block.kind {
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
    let extension = args
        .file
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut timings = Vec::new();
    for _ in 0..args.repeat.max(1) {
        let start = Instant::now();
        if extension == "pdf" {
            let cancel = Cancel::new();
            let options = ReflowOptions::new(std::env::temp_dir().join("cadmus-bench-out"));
            let _ = reflow(&args.file, &options, &cancel)?;
        } else {
            let bytes = std::fs::read(&args.file)?;
            let _ = parse_txt_bytes(&bytes, &TxtOptions::default())?;
        }
        timings.push(start.elapsed());
    }
    timings.sort();
    println!(
        "{} 次：中位 {:?}",
        timings.len(),
        timings[timings.len() / 2]
    );
    Ok(ExitCode::SUCCESS)
}

fn parse_page_range(spec: Option<&str>) -> Result<Option<(u32, u32)>> {
    let Some(spec) = spec else {
        return Ok(None);
    };
    let (start, end) = match spec.split_once('-') {
        Some((start, end)) => (start, end),
        None => (spec, spec),
    };
    let start: u32 = start
        .trim()
        .parse()
        .map_err(|_| Error::Unsupported(format!("页范围格式错误：{spec}")))?;
    let end: u32 = end
        .trim()
        .parse()
        .map_err(|_| Error::Unsupported(format!("页范围格式错误：{spec}")))?;
    if start == 0 || start > end {
        return Err(Error::Unsupported(format!("页范围无效：{spec}")));
    }
    Ok(Some((start, end)))
}
