use shared::response::ApiResponse;

pub fn normalize_server_path(path: &str) -> Result<String, ApiResponse> {
    let normalized = path.trim().trim_start_matches('/').replace('\\', "/");

    if normalized.is_empty()
        || normalized
            .split('/')
            .any(|seg| seg.is_empty() || seg == "." || seg == "..")
        || normalized.chars().any(char::is_control)
    {
        return Err(ApiResponse::error("invalid path"));
    }

    Ok(normalized)
}

pub fn validate_file_name(name: &str) -> Result<(), ApiResponse> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.chars().any(char::is_control)
    {
        return Err(ApiResponse::error("invalid file name"));
    }

    Ok(())
}

pub fn validate_account_label(label: &str) -> Result<(), ApiResponse> {
    if label.is_empty()
        || label.len() > 64
        || !label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        || label == "."
        || label == ".."
    {
        return Err(ApiResponse::error("invalid account label"));
    }

    Ok(())
}
