# 后端架构重构：api.rs 拆分 + 分层

## 目标结构
```
src/
├── main.rs / lib.rs / state.rs / app.rs
├── http/{mod,error,auth}.rs
├── infra/{mod,time,paths,fs,text}.rs
├── features/
│   ├── projects/{mod,api,files,store}.rs
│   ├── characters/{mod,api,store}.rs
│   ├── models/{mod,api,store}.rs
│   └── chat/{mod,api}.rs
├── llm/{mod,config,protocol,json}.rs
└── watch.rs
```

## 步骤
- [ ] A. infra + http/error 抽取；统一 4 处原子写；解除 llm→api 依赖倒置
- [ ] B. state.rs + app.rs（AppState/build_router/startup/health/OpenAPI 测试）
- [ ] C. features/projects（api/files/store 拆分，删 api.rs）
- [ ] D. features/characters + features/models
- [ ] E. features/chat 归位
- [ ] F. llm.rs 拆分（mod/config/protocol/json）
- [ ] G. lib.rs + main.rs 薄壳 + 集成测试（auth/路由）
- [ ] H. tracing（依赖 + 初始化 + TraceLayer + eprintln 替换）
- [ ] I. 文档同步 + 全量验证 + 提交

## 验证（每步）
cargo test → tools/smoke-api.sh → （前端相关时）ui-check.mjs

## 明确不做
- 不引 repository trait / DI；ConfigStore 缓存（本地小文件，收益不抵失效复杂度）
- 错误体加 code 字段（无消费方分支，属于无需求抽象）
