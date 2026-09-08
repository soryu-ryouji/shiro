// 模型输出的 JSON 容错解析（结构化输出场景共用）。

/// 模型输出的 JSON 容错解析（参考 pi-ai 的流式 JSON 容错思路）：
/// 剥 ```json 代码围栏 → 截取首个 {/[ 到末个 }/] → serde 解析。
pub fn extract_json(text: &str) -> Result<serde_json::Value, String> {
    let mut s = text.trim();
    // 剥代码围栏：```json ... ``` 或 ``` ... ```
    if let Some(rest) = s.strip_prefix("```") {
        let rest = rest.split_once('\n').map(|(_, r)| r).unwrap_or(rest);
        s = rest.trim_end().trim_end_matches("```").trim();
    }
    let start = s.find(['{', '[']).ok_or("输出中未找到 JSON")?;
    let end_brace = s.rfind('}');
    let end_bracket = s.rfind(']');
    let end = end_brace
        .zip(end_bracket)
        .map(|(a, b)| a.max(b))
        .or(end_brace)
        .or(end_bracket)
        .ok_or("输出中未找到 JSON 结束符")?;
    if end <= start {
        return Err("JSON 边界异常".into());
    }
    let json_str = &s[start..=end];
    serde_json::from_str(json_str).map_err(|e| format!("JSON 解析失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_tolerant() {
        let v = extract_json("```json\n{\"a\": 1}\n```").unwrap();
        assert_eq!(v["a"], 1);
        let v = extract_json("前置说明 {\"a\": [1,2]} 后置说明").unwrap();
        assert_eq!(v["a"].as_array().unwrap().len(), 2);
        let v = extract_json("废话一段\n[{\"x\": \"y\"}]\n结尾").unwrap();
        assert_eq!(v[0]["x"], "y");
        assert!(extract_json("完全没有 json").is_err());
        assert!(extract_json("```json\n{\"a\": 1").is_err()); // 截断
    }
}
