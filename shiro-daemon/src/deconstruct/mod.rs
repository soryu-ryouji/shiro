// 拆书·角色卡提炼工作流（规范见 docs/工具实现/角色卡提炼如何实现.md）：
// chunk 切块 / probe 素材探测 / evidence 证据层 / prompts 提示词 / verify 引文回查 /
// engine 任务引擎 / api HTTP 路由。

pub mod api;
pub mod chunk;
pub mod engine;
pub mod evidence;
pub mod prompts;
pub mod probe;
pub mod settings;
pub mod verify;
