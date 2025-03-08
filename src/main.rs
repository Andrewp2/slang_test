use bevy::app::Startup;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::render::render_graph::{
    Node, NodeRunError, RenderGraph, RenderGraphContext, RenderLabel,
};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderContext, RenderDevice};
use bevy::render::{Render, RenderApp};
use bytemuck;
use crossbeam_channel::{Receiver, Sender};
use std::convert::TryInto;

use serde_json::Value;
use std::fs;

// === Existing Compute Pipeline Resources === //

#[derive(Resource)]
struct ComputeBuffer {
    buffer: bevy::render::render_resource::Buffer,
}

use std::collections::HashMap;

#[derive(Resource)]
struct ComputePipelineResource {
    pipeline_id: CachedComputePipelineId,
    bind_group_layout: bevy::render::render_resource::BindGroupLayout,
    // Maps parameter names to their binding numbers as read from reflection.json.
    bindings: HashMap<String, u32>,
}

// === New Readback Resources === //

// A CPU–accessible buffer that we will copy our compute output into
#[derive(Resource)]
struct ReadbackBufferResource {
    buffer: bevy::render::render_resource::Buffer,
}

// Channels for sending readback data from the render world to the main world
#[derive(Resource)]
struct RenderWorldSender(Sender<Vec<f32>>);

#[derive(Resource)]
struct MainWorldReceiver(Receiver<Vec<f32>>);

// === Compute Shader Plugin (modified slightly to add readback setup) === //

pub struct ComputeShaderPlugin;

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct ComputeNodeLabel;

impl Plugin for ComputeShaderPlugin {
    fn build(&self, app: &mut App) {
        // Setup our compute buffer and pipeline in the render world
        let render_app = app.sub_app_mut(RenderApp);
        render_app.add_systems(
            Render,
            setup_compute_buffer.run_if(|res: Option<Res<ComputeBuffer>>| res.is_none()),
        );
        render_app.add_systems(
            Render,
            setup_compute_pipeline
                .run_if(|res: Option<Res<ComputePipelineResource>>| res.is_none()),
        );
        // Also create the readback buffer in the render world
        render_app.add_systems(
            Render,
            setup_readback_buffer.run_if(|res: Option<Res<ReadbackBufferResource>>| res.is_none()),
        );

        // Add the compute node to the render graph as before…
        let mut render_graph = render_app.world_mut().resource_mut::<RenderGraph>();
        render_graph.add_node(ComputeNodeLabel, ComputeNode::default());
        render_graph.add_node_edge(ComputeNodeLabel, bevy::render::graph::CameraDriverLabel);

        // Add a render system to do the GPU readback after the compute node has run.
        render_app.add_systems(Render, gpu_readback_system);
    }
}

fn setup_compute_buffer(render_device: Res<RenderDevice>, mut commands: Commands) {
    let data = [0.0f32; 6];
    let buffer = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("Compute Output Buffer"),
        contents: bytemuck::cast_slice(&data),
        usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
    });
    commands.insert_resource(ComputeBuffer { buffer });
}

fn get_buffer_bindings_for_shader(shader_name: &str) -> HashMap<String, u32> {
    let mut bindings = HashMap::new();
    let reflection_path = "assets/compiled_shaders/reflection.json";
    let contents = fs::read_to_string(reflection_path).unwrap_or_else(|err| {
        error!(
            "Failed to read {}: {}. Falling back to empty bindings.",
            reflection_path, err
        );
        String::new()
    });
    if contents.is_empty() {
        error!("Reflection file {} is empty.", reflection_path);
        return bindings;
    }
    let json: Value = serde_json::from_str(&contents).unwrap_or_else(|err| {
        error!(
            "Failed to parse {}: {}. Falling back to empty bindings.",
            reflection_path, err
        );
        Value::Null
    });
    if let Some(arr) = json.as_array() {
        for item in arr {
            if item.get("shader_name").and_then(|v| v.as_str()) == Some(shader_name) {
                if let Some(parameters) = item.get("parameters").and_then(|v| v.as_array()) {
                    for param in parameters {
                        if let Some(param_name) = param.get("name").and_then(|v| v.as_str()) {
                            if let Some(binding) = param
                                .get("resource")
                                .and_then(|r| r.get("binding"))
                                .and_then(|v| v.as_u64())
                            {
                                bindings.insert(param_name.to_string(), binding as u32);
                            }
                        }
                    }
                }
            }
        }
    }
    bindings
}

fn setup_compute_pipeline(
    render_device: Res<RenderDevice>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut pipeline_cache: ResMut<PipelineCache>,
) {
    // Read all bindings for the shader "simple_compute"
    let bindings = get_buffer_bindings_for_shader("simple_compute");
    info!("Bindings for simple_compute: {:?}", bindings);

    // Create layout entries from the mapping.
    let layout_entries: Vec<BindGroupLayoutEntry> = bindings
        .iter()
        .map(|(_name, &binding)| {
            BindGroupLayoutEntry {
                binding,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    // For simplicity, assume each buffer is 6 floats long.
                    min_binding_size: BufferSize::new((6 * std::mem::size_of::<f32>()) as u64),
                },
                count: None,
            }
        })
        .collect();

    let bind_group_layout =
        render_device.create_bind_group_layout("compute_shader_bind_group_layout", &layout_entries);

    let shader_handle = asset_server.load("compiled_shaders/simple_compute_0.spv");
    let _load_state = asset_server.get_load_state(&shader_handle);
    let pipeline_id = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some("simple_compute_pipeline".into()),
        layout: vec![bind_group_layout.clone()],
        shader: shader_handle.clone(),
        shader_defs: vec![],
        push_constant_ranges: vec![],
        entry_point: "main".into(),
    });
    commands.insert_resource(ComputePipelineResource {
        pipeline_id,
        bind_group_layout,
        bindings, // Save the mapping for use in the compute node.
    });
}

// Create a CPU–accessible buffer for readback.
fn setup_readback_buffer(render_device: Res<RenderDevice>, mut commands: Commands) {
    let size = (6 * std::mem::size_of::<f32>()) as u64;
    let buffer = render_device.create_buffer(&BufferDescriptor {
        label: Some("Readback Buffer"),
        size,
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    commands.insert_resource(ReadbackBufferResource { buffer });
}

// === Render Node for Compute Dispatch (unchanged) === //

#[derive(Default)]
struct ComputeNode;

impl Node for ComputeNode {
    fn run(
        &self,
        _graph_context: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let compute_pipeline = match world.get_resource::<ComputePipelineResource>() {
            Some(resource) => resource,
            None => return Ok(()),
        };
        let compute_buffer = world.resource::<ComputeBuffer>();
        let render_device = world.resource::<RenderDevice>();
        let pipeline_cache = world.resource::<PipelineCache>();

        if let Some(pipeline) = pipeline_cache.get_compute_pipeline(compute_pipeline.pipeline_id) {
            // Build bind group entries based on the mapping.
            // For each parameter name, choose the corresponding GPU resource.
            // Here, we assume only "outputBuffer" exists.
            let mut entries = Vec::new();
            if let Some(&binding) = compute_pipeline.bindings.get("outputBuffer") {
                entries.push(BindGroupEntry {
                    binding,
                    resource: compute_buffer.buffer.as_entire_binding(),
                });
            } else {
                error!("No binding found for 'outputBuffer'");
            }
            let bind_group = render_device.create_bind_group(
                "compute_shader_bind_group",
                &compute_pipeline.bind_group_layout,
                &entries,
            );
            let mut pass =
                render_context
                    .command_encoder()
                    .begin_compute_pass(&ComputePassDescriptor {
                        label: Some("compute_shader_pass"),
                        timestamp_writes: Default::default(),
                    });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(6, 1, 1);
        }
        // Schedule a copy from the GPU buffer to the readback buffer.
        {
            let readback_buffer = world.resource::<ReadbackBufferResource>();
            render_context.command_encoder().copy_buffer_to_buffer(
                &compute_buffer.buffer,
                0,
                &readback_buffer.buffer,
                0,
                (6 * std::mem::size_of::<f32>()) as u64,
            );
        }
        Ok(())
    }
}

fn gpu_readback_system(
    render_device: Res<RenderDevice>,
    readback_buffer: Option<Res<ReadbackBufferResource>>,
    sender: Res<RenderWorldSender>,
) {
    // Ensure we have the readback buffer.
    let readback_buffer = match readback_buffer {
        Some(rb) => rb,
        None => {
            error!("No readback buffer has been created");
            return;
        }
    };

    let slice = readback_buffer.buffer.slice(..);
    let (tx, rx) = crossbeam_channel::unbounded::<Result<(), ()>>();
    slice.map_async(MapMode::Read, move |result| {
        let _ = tx.send(result.map_err(|_| ()));
    });
    render_device.poll(Maintain::wait()).panic_on_timeout();

    if rx.recv().unwrap().is_err() {
        eprintln!("Failed to map readback buffer");
        return;
    }

    {
        // Process the mapped data in a scoped block.
        let data = slice.get_mapped_range();
        let values: Vec<f32> = data
            .chunks(std::mem::size_of::<f32>())
            .map(|chunk| {
                let bytes: [u8; 4] = chunk.try_into().expect("should be a f32");
                f32::from_ne_bytes(bytes)
            })
            .collect();

        // Instead of panicking if sending fails, log a warning.
        if let Err(err) = sender.0.send(values) {
            warn!("Failed to send readback data: {:?}", err);
        }
    } // `data` is dropped here, allowing unmap() to work correctly.

    readback_buffer.buffer.unmap();
}

// === Main World System to Print the GPU Readback Data === //

fn print_readback_data_system(receiver: Res<MainWorldReceiver>) {
    if let Ok(values) = receiver.0.try_recv() {
        println!("GPU readback values: {:?}", values);
    }
}

// === Main App Setup === //

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
}

fn main() {
    // Create a channel for transferring data from the render world to the main world.
    let (tx, rx) = crossbeam_channel::unbounded::<Vec<f32>>();
    // Clone the sender so it can be inserted in both worlds.
    let tx_render = tx.clone();

    let mut app = App::new();
    // Insert the channel resource into the main world.
    app.insert_resource(MainWorldReceiver(rx))
        .insert_resource(RenderWorldSender(tx));

    // Add your plugins and systems.
    app.add_plugins(DefaultPlugins.set(LogPlugin {
        level: bevy::log::Level::DEBUG,
        filter: "error,wgpu_core=warn,wgpu_hal=warn".into(),
        ..Default::default()
    }))
    .add_plugins(ComputeShaderPlugin)
    .add_systems(Startup, setup_camera)
    .add_systems(Update, print_readback_data_system);

    // Insert the cloned sender into the render world.
    app.sub_app_mut(RenderApp)
        .insert_resource(RenderWorldSender(tx_render));

    app.run();
}
