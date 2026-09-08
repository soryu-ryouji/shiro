mod api;
mod assets;
mod chat;
mod error;
mod infra;
mod llm;
mod model_api;
mod watch;

use api::AppState;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "shiro-daemon", version, about = "shiro 后端服务")]
struct Cli {
    /// 监听端口
    #[arg(long, default_value_t = 27571)]
    port: u16,

    /// 监听地址（桌面版默认环回；局域网访问传 0.0.0.0）
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// 前端静态资源目录（存在则挂载，SPA 回退 index.html）
    #[arg(long)]
    serve_folder: Option<PathBuf>,

    /// 输出 OpenAPI schema 到 stdout 并退出
    #[arg(long)]
    dump_openapi: bool,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let state = AppState {
        token: std::env::var("SHIRO_TOKEN").unwrap_or_else(|_| {
            let generated = format!("{:x}", rand_u128());
            eprintln!("[shiro-daemon] SHIRO_TOKEN 未设置，已生成临时 token: {generated}");
            generated
        }),
        watch_hub: Default::default(),
        chat: Default::default(),
    };

    let (router, doc) = api::build_router(state.clone(), cli.serve_folder);

    if cli.dump_openapi {
        println!("{}", serde_json::to_string_pretty(&doc).unwrap());
        return;
    }

    // 启动时不自动续跑：重启前运行中的任务标记为「已中断」，等用户点「继续」（不经确认不烧 token）

    let addr = format!("{}:{}", cli.host, cli.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("绑定 {addr} 失败: {e}"));
    println!("[shiro-daemon] listening on http://{addr}");
    axum::serve(listener, router).await.unwrap();
}

/// 不引入 rand 依赖，用系统时间 + 进程 id 拼一个临时随机值（仅用于无 token 的手动调试场景）。
fn rand_u128() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    nanos ^ ((std::process::id() as u128) << 64)
}
