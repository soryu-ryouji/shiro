# 模型接入设计

模型接入层在 shiro-daemon 内实现，为取名、拆解、写作辅助等能力提供统一的 LLM 调用。原则见 [architecture.md](../architecture.md) 核心原则 5。

## Provider 抽象

- daemon 内定义统一 provider trait（对话补全 + 流式输出），各上游各自实现
- 首选 OpenAI 兼容协议（覆盖大多数模型服务），Anthropic 等非兼容协议单独实现
- 上游调用用 reqwest，流式响应用 SSE 转发给前端

## 配置

provider 配置与 API key 存 `~/.config/shiro/config.toml`（见[存储设计](storage.md)）：

- 支持多个 provider 档案（base_url、api_key、model），可指定默认档案
- 按任务类型路由：规划 / 正文 / 审核等环节可各自绑定模型档位（未配置时用默认档案），允许「强模型跑规划、性价比模型跑审核」
- app 内提供配置界面，写入 config.toml；daemon 重读生效
- 密钥只在 daemon 侧使用：上游请求由 daemon 发出，前端永远拿不到 key

## 请求路径

```text
前端 ── REST/SSE ──> shiro-daemon ── HTTPS ──> 上游 LLM API
        （带凭据）        （带 API key）
```

- 生成类端点统一走 SSE，事件名与 schema 契约固化（OpenAPI + 契约测试）
- 局域网访问时移动端同样经 daemon 调用，行为与桌面端一致

## 能力场景

| 能力 | 说明 |
| ---- | ---- |
| 取作品名 | 结合题材、平台、套路库生成候选名 |
| 取人名 | 结合背景设定生成人物名 |
| 拆解 | 对输入文本做结构化拆解（推进模式、人物、套路命中），结果入库可引用 |
| 角色提炼 | 外部剧本 → 角色档案包（深卡），见 [角色卡提炼工作流](../工具实现/角色卡提炼如何实现.md) |

- 提示词模板由 daemon 侧管理（内置 + 后续可配置），前端只传业务参数（如选中的题材、文本），不拼提示词
- 拆解结果写入内容型库（用户自定义区），供编辑器内引用

## 当前实现（V1）

已落地（`shiro-daemon/src/llm.rs` + `src/model_api.rs`）：

- **多档案**：config.toml 的 `[llm]` 段存默认档案 key + `[[llm.profiles]]` 档案数组（key / base_url / api_key / model / protocol）。旧单字段 `[llm]` 配置读取时自动迁移合成单档案（key 由端点反查预设），下次写盘即新结构。引擎（拆解等任务）使用默认档案，改动即生效无需重启
- **配置界面**：Model 面板按功能分层——注册分区「模型导入」（供应商 + 模型可检索下拉 + API Key 三行表单，保存即注册/覆盖同 key 档案）；管理分区「模型管理」（已注册档案列表：供应商 · 当前模型 · Key 掩码状态，可编辑 / 删除 / **测试连接**（POST profiles/{key}/test：发最小真实调用 ping，验证端点 + 密钥 + 模型 + 协议；429 视为配置有效）。制作任务用哪个档案（默认指定）在角色制作的设置面板选择，不在本面板。供应商预设含完整内置模型清单（学 pi 内置目录随版本固化，来源 pi providers/data），端点与协议是预设内部实现，不暴露给用户。预设一览：DeepSeek / Kimi 国内·国际 / Kimi Code / Qwen 订阅国内·国际（token-plan 网关，多厂商：qwen3.8-max 等）/ 智谱 / 通义 / 硅基流动 / OpenAI / OpenRouter
- **思考强度（可选）**：档案级 `thinking`（low/medium/high，缺省不发参数）——openai → `reasoning_effort`，anthropic → `thinking.budget_tokens`（2048/8192/12288，须 < max_tokens）；仅对支持推理参数的模型生效，UI 提供选择与连接测试验证
- **双协议**：OpenAI 兼容（chat/completions，Bearer）与 Anthropic Messages（/v1/messages，x-api-key + anthropic-version，system 顶层字段，max_tokens 必填 16384）——后者供 Kimi Code（https://api.kimi.com/coding）等使用
- **非流式调用**：拆解任务是「单轮结构化输出」，完整响应一次解析即可；曾有 SSE 流式 + 实时输出窗格，因调用日志（请求/响应全文持久化）覆盖而移除（废弃路径不留）
- **安全**：Key 不回传明文（只回掩码如 `sk-…3456`），仅用户修改时提交新值；密钥只在 daemon 侧使用，上游请求由 daemon 发出，前端永远拿不到 key
- **传输**：gzip/brotli/deflate/zstd 解压全开（DeepSeek 经 Cloudflare 会在冷连接阶段返回 zstd——缺 zstd 特性时表现为「开头的调用必失败，后续恢复」）
- **重试**：429/408/5xx/网络错误指数退避（1s/2s/4s 封顶 8s，默认最多 3 次）；其余 4xx 直接失败（两协议共用同一重试循环）
- **JSON 容错解析**：剥代码围栏 + 截取首尾括号（供拆解笔记等结构化输出场景）

待后续：按任务类型路由（规划/正文/审核各自绑定档案）、token 用量统计。

全局运行参数：`max_concurrency`（1-8，默认 4）——所有 AI 调用的并发闸门，拆解管线的笔记与生成波次共用。与模型档案选择（[llm] default）、切块目标长度（[deconstruct] segment_chars）一起，经角色制作设置 API `GET/PUT /api/v1/db/deconstruct/settings` 读写（前端入口在角色制作视图的设置面板）。

## 实现期待细化

重试与超时策略、多请求并发上限、按 provider 的限速、token 用量统计。
