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

/// Known tokens accepted in install templates.
const RENAME_TOKENS: [&str; 5] = [
    "{workshop_id}",
    "{app_id}",
    "{ext}",
    "{basename}",
    "{title_slug}",
];

/// Validate a game preset's install rules at save time so admins get immediate
/// feedback. The helper re-validates rendered destinations at download time —
/// this is the friendly first line, not the security boundary.
pub fn validate_game_presets(presets: &[crate::settings::GamePreset]) -> Result<(), ApiResponse> {
    for preset in presets {
        // install_path must be a sane relative path.
        normalize_server_path(&preset.install_path).map_err(|_| {
            ApiResponse::error(format!(
                "preset '{}' has an invalid install path",
                preset.name
            ))
        })?;

        for rule in &preset.r#match {
            if rule.glob.trim().is_empty() {
                return Err(ApiResponse::error(format!(
                    "preset '{}' has an empty glob",
                    preset.name
                )));
            }
            if let Some(template) = &rule.rename {
                validate_rename_template(template).map_err(|reason| {
                    ApiResponse::error(format!("preset '{}': {reason}", preset.name))
                })?;
            }
        }
        for generated in &preset.generated_files {
            validate_template_path(&generated.path).map_err(|reason| {
                ApiResponse::error(format!("preset '{}': {reason}", preset.name))
            })?;
        }
        for extract in &preset.extract_files {
            if !extract.format.eq_ignore_ascii_case("gma") {
                return Err(ApiResponse::error(format!(
                    "preset '{}' has unsupported extract format '{}'",
                    preset.name, extract.format
                )));
            }
            if extract.glob.trim().is_empty() {
                return Err(ApiResponse::error(format!(
                    "preset '{}' has an empty extract glob",
                    preset.name
                )));
            }
            validate_template_path(&extract.to).map_err(|reason| {
                ApiResponse::error(format!("preset '{}': {reason}", preset.name))
            })?;
        }
        for scan in &preset.scan {
            normalize_server_path(&scan.path).map_err(|_| {
                ApiResponse::error(format!("preset '{}' has an invalid scan path", preset.name))
            })?;
            if scan.extensions.iter().any(|ext| {
                ext.is_empty()
                    || ext.contains('.')
                    || ext.contains('/')
                    || ext.contains('\\')
                    || ext.chars().any(char::is_control)
            }) {
                return Err(ApiResponse::error(format!(
                    "preset '{}' has an invalid scan extension",
                    preset.name
                )));
            }
        }
    }
    Ok(())
}

fn validate_rename_template(template: &str) -> Result<(), String> {
    validate_template_path(template).map_err(|reason| reason.replace("template", "rename template"))
}

fn validate_template_path(template: &str) -> Result<(), String> {
    // Strip known tokens, then ensure no stray braces remain (i.e. unknown token).
    let mut stripped = template.to_string();
    for token in RENAME_TOKENS {
        stripped = stripped.replace(token, "");
    }
    if stripped.contains('{') || stripped.contains('}') {
        return Err(format!("template '{template}' has an unknown token"));
    }
    // The literal portion must form a safe relative path (subdirs allowed).
    let literal = stripped.trim_matches('/');
    if template.starts_with('/') {
        return Err(format!("template '{template}' must be relative"));
    }
    if literal
        .split('/')
        .any(|seg| seg == ".." || seg.chars().any(char::is_control) || seg.contains(':'))
    {
        return Err(format!("template '{template}' has an invalid segment"));
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
