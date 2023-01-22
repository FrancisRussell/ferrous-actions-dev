use crate::{node, Error};
use beef::Cow;
use node::path::Path;

pub fn get_action_name() -> Cow<'static, str> {
    "ferrous-actions".into()
}

#[allow(clippy::unnecessary_wraps)]
pub fn get_action_share_dir() -> Result<Path, Error> {
    Ok(node::os::homedir()
        .join(".local")
        .join("share")
        .join(get_action_name().as_ref()))
}

#[allow(clippy::unnecessary_wraps)]
pub fn get_action_cache_dir() -> Result<Path, Error> {
    Ok(node::os::homedir().join(".cache").join(get_action_name().as_ref()))
}
