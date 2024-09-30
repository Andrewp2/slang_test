use std::path::PathBuf;

use bevy::{
    asset::{io::Reader, AssetLoader, AssetPath, AsyncReadExt, LoadContext},
    ecs::system::SystemState,
    prelude::*,
    render::RenderApp,
    utils::HashMap,
};
use serde::{Deserialize, Serialize};

use slang::*;

pub struct SlangPlugin;

impl Plugin for SlangPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SlangRegistry>()
            .init_asset::<SlangShader>()
            .register_asset_loader(SlangLoader);
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.init_resource::<SlangRegistry>();
    }
}

/// Holds Slang shader handles so the file watcher will watch for updates and cause a new SPIR-V file to be generated when changes are made.
#[derive(Resource, Default)]
pub struct SlangRegistry(HashMap<PathBuf, Handle<SlangShader>>);

impl SlangRegistry {
    /// Accepted profiles are:
    /// * sm_{4_0,4_1,5_0,5_1,6_0,6_1,6_2,6_3,6_4,6_5,6_6}
    /// * glsl_{110,120,130,140,150,330,400,410,420,430,440,450,460}
    /// Additional profiles that include -stage information:
    /// * {vs,hs,ds,gs,ps}_<version>
    pub fn load<'a>(
        &mut self,
        path: impl Into<AssetPath<'a>> + std::marker::Copy,
        asset_server: &AssetServer,
        profile: &str,
    ) -> Handle<Shader> {
        let p: PathBuf = path.into().into();
        let profile = String::from(profile);

        #[cfg(not(target_arch = "wasm32"))]
        {
            let h = asset_server.load_with_settings(path, move |s: &mut SlangSettings| {
                s.profile = profile.clone();
            });
            self.0.insert(p.clone(), h);
        }
        // Instead of loading a file with .spv extension, we directly return the handle to the SlangShader
        asset_server.load(p)
    }

    /// Accepted profiles are:
    /// * sm_{4_0,4_1,5_0,5_1,6_0,6_1,6_2,6_3,6_4,6_5,6_6}
    /// * glsl_{110,120,130,140,150,330,400,410,420,430,440,450,460}
    /// Additional profiles that include -stage information:
    /// * {vs,hs,ds,gs,ps}_<version>
    pub fn load_from_world<'a>(
        path: impl Into<AssetPath<'a>> + std::marker::Copy,
        world: &mut World,
        profile: &str,
    ) -> Handle<Shader> {
        let mut system_state: SystemState<(Res<AssetServer>, ResMut<SlangRegistry>)> =
            SystemState::new(world);
        let (asset_server, mut slang) = system_state.get_mut(world);
        slang.load(path, &asset_server, profile)
    }
}

#[derive(Asset, TypePath, Debug)]
pub struct SlangShader(pub Vec<u8>);

#[derive(Default)]
struct SlangLoader;

#[derive(Default, Serialize, Deserialize)]
struct SlangSettings {
    profile: String,
}

impl AssetLoader for SlangLoader {
    type Asset = SlangShader;
    type Settings = SlangSettings;
    type Error = std::io::Error;

    fn extensions(&self) -> &[&str] {
        &["slang"]
    }

    async fn load<'a>(
        &'a self,
        reader: &'a mut Reader<'_>,
        settings: &'a SlangSettings,
        load_context: &'a mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut shader_bytes: Vec<u8> = vec![];
        reader.read_to_end(&mut shader_bytes).await?;
        let shader_source = String::from_utf8(shader_bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let global_session = GlobalSession::new().map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("Slang error: {:?}", e))
        })?;
        let profile_id = global_session.find_profile(&settings.profile);
        let target_desc = TargetDescBuilder::default()
            .format(CompileTarget::SPIRV)
            .profile(profile_id);
        let session_desc = SessionDescBuilder::default().targets(&[target_desc]);
        let mut session = global_session.create_session(session_desc).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("Slang error: {:?}", e))
        })?;
        let source_blob = Blob::from(shader_source);
        let module_name = "shader_module";
        let path = load_context.path().to_str().unwrap_or("unknown");
        let mut module = session
            .load_module_from_source(module_name, path, &source_blob)
            .map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::Other, format!("Slang error: {:?}", e))
            })?
            .to_owned();
        let component_type = module.link().map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("Slang error: {:?}", e))
        })?;
        let code_blob = component_type.get_entry_point_code(0, 0).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("Slang error: {:?}", e))
        })?;
        let spirv_code = code_blob.as_slice().to_vec();
        Ok(SlangShader(spirv_code))
    }
}
