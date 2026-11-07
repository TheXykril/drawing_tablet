//! Bind this server's virtual input devices to the captured KDE output.
use anyhow::{bail, Context, Result};
use dt_capture::MonitorInfo;
use dt_input::VirtualTablet;
use serde_json::Value;
use std::{path::Path, time::Duration};

async fn command(program: &str, args: &[&str]) -> Result<Vec<u8>> {
    let output = tokio::time::timeout(
        Duration::from_secs(3),
        tokio::process::Command::new(program)
            .args(args)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .context("Desktop command timed out")??;
    if !output.status.success() {
        bail!(
            "{program}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
}

fn select_output(
    outputs: &[Value],
    monitor: &MonitorInfo,
    override_name: Option<&str>,
) -> Result<String> {
    let enabled: Vec<_> = outputs
        .iter()
        .filter(|o| o["enabled"] == true && o["connected"] == true)
        .collect();
    let name = override_name.or_else(|| {
        monitor
            .mapping_id
            .as_deref()
            .filter(|id| enabled.iter().any(|o| o["name"].as_str() == Some(*id)))
    });
    let matches: Vec<_> = enabled
        .into_iter()
        .filter(|o| {
            if let Some(name) = name {
                return o["name"].as_str() == Some(name);
            }
            monitor.position.is_some_and(|(x, y)| {
                o["pos"]["x"].as_i64() == Some(x as i64) && o["pos"]["y"].as_i64() == Some(y as i64)
            })
        })
        .collect();
    if matches.len() != 1 {
        bail!("Cannot uniquely identify the selected display; enter its connector name in Input monitor");
    }
    Ok(matches[0]["name"]
        .as_str()
        .context("Output has no name")?
        .to_owned())
}

async fn set_property(event: &str, property: &str, signature: &str, value: &str) -> Result<()> {
    let path = format!("/org/kde/KWin/InputDevice/{event}");
    command(
        "busctl",
        &[
            "--user",
            "set-property",
            "org.kde.KWin",
            &path,
            "org.kde.KWin.InputDevice",
            property,
            signature,
            value,
        ],
    )
    .await?;
    Ok(())
}

fn event_node(path: &Path) -> Option<String> {
    std::fs::read_dir(path)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .find(|name| name.starts_with("event"))
}

pub async fn map_tablet(
    monitor: &MonitorInfo,
    tablet: &mut VirtualTablet,
    override_name: Option<&str>,
) -> Result<String> {
    if !std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .any(|s| s.eq_ignore_ascii_case("kde"))
    {
        bail!("Automatic mapping is currently supported on KDE; map Drawing Tablet Pen and Drawing Tablet Touch in desktop settings");
    }
    let bytes = command("kscreen-doctor", &["-j"])
        .await
        .context("kscreen-doctor is required for automatic mapping")?;
    let data: Value = serde_json::from_slice(&bytes)?;
    let output = select_output(
        data["outputs"].as_array().context("Missing outputs")?,
        monitor,
        override_name,
    )?;
    let paths = tablet.device_syspaths()?;
    for (index, path) in paths.iter().enumerate() {
        // udev and KWin discover a new uinput device asynchronously.
        let mut last_error = None;
        let mut mapped = false;
        for _ in 0..30 {
            if let Some(event) = event_node(path) {
                let result = async {
                    if index == 0 {
                        set_property(&event, "mapToWorkspace", "b", "false").await?;
                        set_property(&event, "tabletToolIsRelative", "b", "false").await?;
                    }
                    set_property(&event, "outputName", "s", &output).await
                }
                .await;
                match result {
                    Ok(()) => {
                        mapped = true;
                        break;
                    }
                    Err(e) => last_error = Some(e),
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if !mapped {
            bail!(
                "Could not map {}: {}",
                if index == 0 { "pen" } else { "touch" },
                last_error
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "Device not registered".into())
            );
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn monitor(position: Option<(i32, i32)>, mapping_id: Option<&str>) -> MonitorInfo {
        MonitorInfo {
            node_id: 1,
            width: 1920,
            height: 1080,
            position,
            mapping_id: mapping_id.map(str::to_owned),
            restore_token: None,
            _session: std::sync::Arc::new(()),
        }
    }
    fn outputs() -> Vec<Value> {
        vec![
            json!({"name":"DP-1","enabled":true,"connected":true,"pos":{"x":-1920,"y":0}}),
            json!({"name":"DP-2","enabled":true,"connected":true,"pos":{"x":0,"y":0}}),
        ]
    }
    #[test]
    fn selects_origin_instead_of_identical_resolution() {
        assert_eq!(
            select_output(&outputs(), &monitor(Some((-1920, 0)), None), None).unwrap(),
            "DP-1"
        );
    }
    #[test]
    fn missing_or_ambiguous_metadata_requires_explicit_choice() {
        assert!(select_output(&outputs(), &monitor(None, None), None).is_err());
        let mut duplicate = outputs();
        duplicate[1]["pos"]["x"] = json!(-1920);
        assert!(select_output(&duplicate, &monitor(Some((-1920, 0)), None), None).is_err());
        assert_eq!(
            select_output(&duplicate, &monitor(Some((-1920, 0)), Some("DP-2")), None).unwrap(),
            "DP-2"
        );
    }
    #[test]
    fn manual_override_is_validated() {
        assert_eq!(
            select_output(&outputs(), &monitor(None, None), Some("DP-2")).unwrap(),
            "DP-2"
        );
        assert!(select_output(&outputs(), &monitor(None, None), Some("DP-9")).is_err());
        let mut disabled = outputs();
        disabled[1]["enabled"] = json!(false);
        assert!(select_output(&disabled, &monitor(None, None), Some("DP-2")).is_err());
    }
}
