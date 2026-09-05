# shiro 项目数据存储方案

## 应用缓存与配置

Electron 软件本身的缓存留在默认的位置，但 shiro 的配置与数据需要放在 `~/.config/shiro/` 目录下

```text
~/.config/shiro/
├── db/                 <- shiro 仓库数据
└── config.toml         <- shiro 项目配置
```
