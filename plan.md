# 删除角色卡管线 + Chat 视图 V1

## 当前步骤
- [ ] 0. 存档提交（未提交改动先 commit）
- [ ] 1. 删除后端 deconstruct 模块（含 api.rs 路由、AppState 字段、assets.rs 常量内联）
- [ ] 2. 删除前端 Craft 相关（CraftView/CraftSettingsDialog/SegmentPicker/PipelineFlow/stores/deconstruct、DatabasePanel 制作分组、database store craft 分类）+ 测试脚本（smoke-deconstruct、ui-check-craft）
- [ ] 3. cargo test + 重固化 openapi.json + gen:types + typecheck
- [ ] 4. 文档更新（删除相关引用，工具实现两篇加存档标注）
- [ ] 5. 提交删除
- [ ] 6. 后端 chat 模块：会话存储（<项目>/.shiro/chat/）+ 会话 CRUD + messages SSE 流 + apply 提案
- [ ] 7. 前端 Chat 视图：导航第二项、stores/chat.ts、ChatView.vue（项目选择 + 会话列表 + 消息流 + 附件 + 提案卡片）
- [ ] 8. 验证：cargo test / typecheck / daemon 手动冒烟（会话 CRUD）
- [ ] 9. Chat 文档（docs/frontend/chat.md）+ model-access/layout 更新
- [ ] 10. 提交功能，删除本 plan.md

## 已定决策
- pipeline 骨架代码全删，文档保留（设计存档）；llm.rs 全保留；LogDialog.vue 保留
- 深卡人物库保留，GENERATION_FILES 内联
- Chat 导航第二位（Project 之后），Database 保留
- 提案 = 整文件覆盖写盘，走确认；会话存 <项目>/.shiro/chat/<id>.json
