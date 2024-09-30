pub mod slang;

use bevy::prelude::*;
use slang::SlangPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(SlangPlugin)
        .run();
}
