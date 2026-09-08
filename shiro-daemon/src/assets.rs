// 内容库资产 API（全局 db，格式见 docs/asset-format.md）：
// 角色卡存 ~/.config/shiro/db/人物/<id>.md，明文 markdown + YAML frontmatter。
// 宽容解析：frontmatter 缺失/损坏/未标记 shiro_asset 类型的文件不进列表，按普通文档对待，不报错。

use crate::api::{
    ApiError, ErrorResponse, bad_request, config_dir, excerpt_from_content, file_mtime,
    internal_error, now_secs,
};
use axum::Json;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use utoipa::ToSchema;

/// 预览正文字数（与项目文稿预览同量级）
const EXCERPT_CHARS: usize = 160;

#[derive(Serialize, ToSchema)]
pub struct CharacterSummary {
    /// 角色 id（单文件：db/人物/ 下文件名去掉 .md；深卡：目录名）
    pub id: String,
    /// 显示名（frontmatter 的 name；缺失回退文件名）
    pub name: String,
    /// 功能角色（主角 / 对手 / 盟友 / …）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// 原型标签（可多个）
    pub archetype: Vec<String>,
    /// 自由标签
    pub tags: Vec<String>,
    /// 来源标注（应用的全局原型，如 db://characters/xxx）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// 深卡标记（目录形态档案包时为 full）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<String>,
    /// 正文预览（剥离 markdown 标记后取开头）
    pub excerpt: String,
    /// 文件修改时间（epoch 秒）
    pub modified: u64,
}

#[derive(Serialize, ToSchema)]
pub struct CharacterListResponse {
    pub characters: Vec<CharacterSummary>,
}

#[derive(Serialize, ToSchema)]
pub struct CardFile {
    /// 子文件名（不带扩展名）：soul / speech_patterns / … / limit
    pub name: String,
    /// 子文件内容（markdown）
    pub body: String,
}

#[derive(Serialize, ToSchema)]
pub struct CharacterDetail {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    pub archetype: Vec<String>,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// 深卡标记（目录形态档案包时为 full）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<String>,
    /// 深卡子文件（单文件简卡为空数组；index 正文已在 body）
    pub files: Vec<CardFile>,
    /// 正文 markdown（单文件卡：frontmatter 之后全文；深卡：index.md 正文）
    pub body: String,
    /// 文件原始内容（含 frontmatter，编辑用；深卡为 index.md 原文）
    pub raw: String,
}

/// 全局人物库目录：~/.config/shiro/db/人物/
pub(crate) fn characters_dir() -> PathBuf {
    config_dir().join("db").join("人物")
}

/// 拆分 frontmatter 与正文：首行 `---` 到下一个整行 `---` 之间为 YAML；未闭合视为无 frontmatter
fn split_frontmatter(content: &str) -> Option<(String, String)> {
    let mut lines = content.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let mut yaml = Vec::new();
    let mut body = Vec::new();
    let mut in_yaml = true;
    for line in lines {
        if in_yaml {
            if line.trim() == "---" {
                in_yaml = false;
                continue;
            }
            yaml.push(line);
        } else {
            body.push(line);
        }
    }
    if in_yaml {
        return None;
    }
    Some((yaml.join("\n"), body.join("\n")))
}

/// YAML 字段宽容取值：字符串或字符串序列都收，其余类型忽略
fn yaml_strings(v: Option<&serde_yaml::Value>) -> Vec<String> {
    match v {
        Some(serde_yaml::Value::String(s)) => vec![s.clone()],
        Some(serde_yaml::Value::Sequence(seq)) => seq
            .iter()
            .filter_map(|i| i.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

/// 从 frontmatter 与正文解析角色摘要：非角色资产（未标记 shiro_asset: character）或 YAML 损坏返回 None
fn parse_character(fm: &str, body: &str, id: &str) -> Option<CharacterSummary> {
    let yaml: serde_yaml::Value = serde_yaml::from_str(fm).ok()?;
    if yaml.get("shiro_asset").and_then(|v| v.as_str()) != Some("character") {
        return None;
    }
    let name = yaml
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| id.to_string());
    Some(CharacterSummary {
        id: id.to_string(),
        name,
        role: yaml
            .get("role")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from),
        archetype: yaml_strings(yaml.get("archetype")),
        tags: yaml_strings(yaml.get("tags")),
        source: yaml
            .get("source")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from),
        depth: yaml
            .get("depth")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from),
        excerpt: excerpt_from_content(body, EXCERPT_CHARS, true),
        modified: 0,
    })
}

/// 扫描目录下的角色卡：单文件 <id>.md 与目录深卡 <id>/index.md 并收，按显示名排序
fn scan_characters(dir: &Path) -> Vec<CharacterSummary> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            // 目录深卡：入口 index.md
            let index = path.join("index.md");
            let Ok(content) = std::fs::read_to_string(&index) else {
                continue;
            };
            let Some((fm, body)) = split_frontmatter(&content) else {
                continue;
            };
            let Some(mut c) = parse_character(&fm, &body, &name) else {
                continue;
            };
            c.modified = file_mtime(&index);
            out.push(c);
            continue;
        }
        if !name.ends_with(".md") {
            continue;
        }
        let Some(id) = name.strip_suffix(".md") else {
            continue;
        };
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some((fm, body)) = split_frontmatter(&content) else {
            continue;
        };
        let Some(mut c) = parse_character(&fm, &body, id) else {
            continue;
        };
        c.modified = file_mtime(&path);
        out.push(c);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 角色 id 合法性：非空、不含路径分隔符、不以 . 开头（防穿越与隐藏文件）
fn valid_asset_id(id: &str) -> bool {
    !id.is_empty() && !id.starts_with('.') && !id.contains(['/', '\\'])
}

fn not_found(message: &str) -> ApiError {
    (
        axum::http::StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            message: message.into(),
        }),
    )
}

/// 全局人物库角色列表
#[utoipa::path(
    get,
    path = "/api/v1/db/characters",
    tag = "db",
    responses(
        (status = 200, description = "角色列表（按显示名排序）", body = CharacterListResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn list_characters() -> Json<CharacterListResponse> {
    Json(CharacterListResponse {
        characters: scan_characters(&characters_dir()),
    })
}

/// 深卡子文件固定顺序（soul → … → limit），缺文件跳过；格式见 docs/asset-format.md 深卡章节
const DEEP_CARD_FILES: [&str; 6] = [
    "soul",
    "speech_patterns",
    "behavior_guide",
    "relationship_dynamics",
    "key_life_events",
    "limit",
];

/// 深卡子文件读取：固定顺序（soul → … → limit），缺文件跳过
fn read_deep_files(dir: &Path) -> Vec<CardFile> {
    DEEP_CARD_FILES
        .iter()
        .filter_map(|name| {
            let body = std::fs::read_to_string(dir.join(format!("{name}.md"))).ok()?;
            Some(CardFile {
                name: (*name).to_string(),
                body,
            })
        })
        .collect()
}

/// 角色详情（单文件简卡：frontmatter 字段 + 正文；目录深卡：index.md + 子文件列表）
#[utoipa::path(
    get,
    path = "/api/v1/db/characters/{id}",
    tag = "db",
    params(
        ("id" = String, Path, description = "角色 id（单文件卡去 .md 的文件名；深卡为目录名）")
    ),
    responses(
        (status = 200, description = "角色详情", body = CharacterDetail),
        (status = 400, description = "非法 id", body = ErrorResponse),
        (status = 404, description = "角色不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn get_character(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<CharacterDetail>, ApiError> {
    if !valid_asset_id(&id) {
        return Err(bad_request("非法的角色 id"));
    }
    // 单文件简卡优先，其次目录深卡（index.md）
    let file_path = characters_dir().join(format!("{id}.md"));
    let dir_path = characters_dir().join(&id);
    if file_path.is_file() {
        let content = std::fs::read_to_string(&file_path).map_err(|_| not_found("角色不存在"))?;
        let Some((fm, body)) = split_frontmatter(&content) else {
            return Err(not_found("角色不存在"));
        };
        let Some(c) = parse_character(&fm, &body, &id) else {
            return Err(not_found("角色不存在"));
        };
        return Ok(Json(CharacterDetail {
            id: c.id,
            name: c.name,
            role: c.role,
            archetype: c.archetype,
            tags: c.tags,
            source: c.source,
            depth: c.depth,
            files: Vec::new(),
            body,
            raw: content,
        }));
    }
    let index = dir_path.join("index.md");
    if index.is_file() {
        let content = std::fs::read_to_string(&index).map_err(|_| not_found("角色不存在"))?;
        let Some((fm, body)) = split_frontmatter(&content) else {
            return Err(not_found("角色不存在"));
        };
        let Some(c) = parse_character(&fm, &body, &id) else {
            return Err(not_found("角色不存在"));
        };
        return Ok(Json(CharacterDetail {
            id: c.id,
            name: c.name,
            role: c.role,
            archetype: c.archetype,
            tags: c.tags,
            source: c.source,
            depth: c.depth,
            files: read_deep_files(&dir_path),
            body,
            raw: content,
        }));
    }
    Err(not_found("角色不存在"))
}

#[derive(Deserialize, ToSchema)]
pub struct SaveCharacterRequest {
    /// 完整文件内容（含 frontmatter）
    pub content: String,
}

#[derive(Serialize, ToSchema)]
pub struct SaveCharacterResponse {
    /// 保存后能否仍解析为角色卡（false = frontmatter 缺失/损坏/未标记类型，该卡不进列表）
    pub parsed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_error: Option<String>,
    /// 保存后重新解析出的摘要（parsed 为 false 时为空）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character: Option<CharacterSummary>,
}

/// 保存角色卡：临时文件 + rename 原子覆盖（同文稿保存）。
/// 宽容策略：解析失败也保存（用户手改中途不丢内容），但返回 parse_error 提示该卡会从列表消失。
#[utoipa::path(
    put,
    path = "/api/v1/db/characters/{id}",
    tag = "db",
    params(
        ("id" = String, Path, description = "角色 id（db/人物/ 下文件名去掉 .md）")
    ),
    request_body = SaveCharacterRequest,
    responses(
        (status = 200, description = "已保存（含解析反馈）", body = SaveCharacterResponse),
        (status = 400, description = "非法 id", body = ErrorResponse),
        (status = 404, description = "角色不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权"),
        (status = 500, description = "写入失败", body = ErrorResponse)
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn save_character(
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(req): Json<SaveCharacterRequest>,
) -> Result<Json<SaveCharacterResponse>, ApiError> {
    if !valid_asset_id(&id) {
        return Err(bad_request("非法的角色 id"));
    }
    // 单文件简卡优先；其次目录深卡（编辑对象是 index.md，写回原位）
    let single = characters_dir().join(format!("{id}.md"));
    let deep = characters_dir().join(&id).join("index.md");
    let path = if single.is_file() {
        single
    } else if deep.is_file() {
        deep
    } else {
        return Err(not_found("角色不存在"));
    };
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &req.content).map_err(internal_error)?;
    std::fs::rename(&tmp, &path).map_err(internal_error)?;

    let parsed = split_frontmatter(&req.content).and_then(|(fm, body)| {
        parse_character(&fm, &body, &id).map(|mut c| {
            c.modified = file_mtime(&path);
            c
        })
    });
    Ok(Json(match parsed {
        Some(c) => SaveCharacterResponse {
            parsed: true,
            parse_error: None,
            character: Some(c),
        },
        None => SaveCharacterResponse {
            parsed: false,
            parse_error: Some(
                "frontmatter 缺失、YAML 损坏或未标记 shiro_asset: character——保存成功，但该卡不会出现在角色列表中"
                    .into(),
            ),
            character: None,
        },
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct CreateCharacterRequest {
    /// 角色显示名（写入 frontmatter 的 name）
    pub name: String,
}

/// 由显示名生成 id：仅保留 ASCII 字母数字与连字符；为空（如纯中文名）时用 character-<时间戳>
pub(crate) fn slug_for(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        format!("character-{}", now_secs())
    } else {
        slug
    }
}

/// 新卡骨架：frontmatter 经 serde_yaml 序列化（名字含引号/冒号也合法），正文预置四节标题
fn skeleton_content(name: &str) -> String {
    #[derive(Serialize)]
    struct Fm<'a> {
        shiro_asset: &'a str,
        name: &'a str,
    }
    let fm = serde_yaml::to_string(&Fm {
        shiro_asset: "character",
        name,
    })
    .unwrap_or_default();
    format!(
        "---\n{fm}---\n\n## 可迁移\n\n### 性格核心\n\n\n\n### 动机与目标模式\n\n\n\n### 说话风格\n\n\n\n### 弧线类型\n\n"
    )
}

/// 新建角色卡（骨架）：同名自动追加 -2/-3 序号；返回可直接进入编辑的详情
#[utoipa::path(
    post,
    path = "/api/v1/db/characters",
    tag = "db",
    request_body = CreateCharacterRequest,
    responses(
        (status = 201, description = "已创建", body = CharacterDetail),
        (status = 400, description = "名称为空", body = ErrorResponse),
        (status = 401, description = "未鉴权"),
        (status = 500, description = "写入失败", body = ErrorResponse)
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn create_character(
    Json(req): Json<CreateCharacterRequest>,
) -> Result<(StatusCode, Json<CharacterDetail>), ApiError> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(bad_request("名称不能为空"));
    }
    let dir = characters_dir();
    std::fs::create_dir_all(&dir).map_err(internal_error)?;
    let base = slug_for(name);
    let mut id = base.clone();
    let mut n = 2;
    while dir.join(format!("{id}.md")).exists() {
        id = format!("{base}-{n}");
        n += 1;
    }
    let content = skeleton_content(name);
    let path = dir.join(format!("{id}.md"));
    std::fs::write(&path, &content).map_err(internal_error)?;
    let body = split_frontmatter(&content)
        .map(|(_, body)| body)
        .unwrap_or_default();
    Ok((
        StatusCode::CREATED,
        Json(CharacterDetail {
            id,
            name: name.to_string(),
            role: None,
            archetype: vec![],
            tags: vec![],
            source: None,
            depth: None,
            files: Vec::new(),
            body,
            raw: content,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("shiro-assets-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn split_frontmatter_cases() {
        assert_eq!(split_frontmatter("没有 frontmatter"), None);
        assert_eq!(split_frontmatter("---\nname: a\n"), None); // 未闭合
        let (fm, body) = split_frontmatter("---\nname: a\n---\n\n正文\n").unwrap();
        assert_eq!(fm, "name: a");
        assert_eq!(body.trim(), "正文");
    }

    #[test]
    fn scan_filters_and_falls_back() {
        let dir = tmp_dir("scan");
        std::fs::write(
            dir.join("valid.md"),
            "---\nshiro_asset: character\nname: 沈青梧\narchetype: [疯批美人, 高门弃女]\nrole: 对手\ntags: [女二]\nsource: db://characters/x\n---\n\n## 可迁移\n\n掌控欲强，以退为进。\n",
        )
        .unwrap();
        std::fs::write(dir.join("plain.md"), "---\ntitle: 随笔\n---\n\n普通文档").unwrap();
        std::fs::write(
            dir.join("broken.md"),
            "---\nshiro_asset: character\nname: [未闭合\n---\n正文",
        )
        .unwrap();
        std::fs::write(dir.join("noname.md"), "---\nshiro_asset: character\n---\n\n无名字段，回退文件名").unwrap();
        std::fs::write(dir.join("notes.txt"), "非 md 不收").unwrap();

        let list = scan_characters(&dir);
        assert_eq!(list.len(), 2, "只收标记为 character 的合法 md");

        let valid = list.iter().find(|c| c.id == "valid").unwrap();
        assert_eq!(valid.name, "沈青梧");
        assert_eq!(valid.archetype, vec!["疯批美人", "高门弃女"]);
        assert_eq!(valid.role.as_deref(), Some("对手"));
        assert_eq!(valid.source.as_deref(), Some("db://characters/x"));
        assert!(valid.excerpt.contains("掌控欲强"));

        let noname = list.iter().find(|c| c.id == "noname").unwrap();
        assert_eq!(noname.name, "noname", "缺 name 回退文件名");
        assert!(noname.modified > 0);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn id_validation() {
        assert!(valid_asset_id("fengpi-meirei"));
        assert!(!valid_asset_id(""));
        assert!(!valid_asset_id("../etc"));
        assert!(!valid_asset_id("a/b"));
        assert!(!valid_asset_id(".hidden"));
    }

    #[test]
    fn scan_deep_card_dir() {
        let dir = tmp_dir("deep");
        let card_dir = dir.join("makima");
        std::fs::create_dir_all(card_dir.join("sub")).unwrap();
        std::fs::write(
            card_dir.join("index.md"),
            "---\nshiro_asset: character\ndepth: full\nname: 玛奇玛\nsource_work:\n  title: 链锯人\n---\n\n## 一句话\n\n温柔的支配者。\n",
        )
        .unwrap();
        std::fs::write(
            card_dir.join("soul.md"),
            "## 核心驱动力\n\n链锯人。\n",
        )
        .unwrap();
        // 无 index.md 的目录不收
        std::fs::create_dir_all(dir.join("empty-dir")).unwrap();

        let list = scan_characters(&dir);
        assert_eq!(list.len(), 1);
        let c = &list[0];
        assert_eq!(c.id, "makima");
        assert_eq!(c.name, "玛奇玛");
        assert_eq!(c.depth.as_deref(), Some("full"));
        assert!(c.excerpt.contains("温柔的支配者"));
        assert!(c.modified > 0);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn slug_rules() {
        assert_eq!(slug_for("Feng Pi Mei Ren"), "feng-pi-mei-ren");
        assert!(slug_for("沈青梧").starts_with("character-"), "纯中文名回退时间戳 id");
    }

    #[test]
    fn skeleton_parses_as_character() {
        let content = skeleton_content("测试: 带冒号与\"引号\"的名");
        let (fm, body) = split_frontmatter(&content).unwrap();
        let c = parse_character(&fm, &body, "test-id").unwrap();
        assert_eq!(c.name, "测试: 带冒号与\"引号\"的名");
        assert!(body.contains("## 可迁移"));
        assert!(body.contains("### 弧线类型"));
    }
}
