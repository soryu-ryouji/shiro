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

- 提示词模板由 daemon 侧管理（内置 + 后续可配置），前端只传业务参数（如选中的题材、文本），不拼提示词
- 拆解结果写入内容型库（用户自定义区），供编辑器内引用

## 实现期待细化

重试与超时策略、多请求并发上限、按 provider 的限速、token 用量统计。
