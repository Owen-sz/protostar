use crate::xdg::{DesktopFile, Icon, IconType};
use color_eyre::eyre::{eyre, Result};
use glam::Quat;
use mint::Vector3;
use nix::unistd::setsid;
use stardust_xr_fusion::{
	client::{Client, FrameInfo, RootHandler},
	core::values::Transform,
	drawable::{MaterialParameter, Model, ResourceID},
	fields::BoxField,
	node::NodeType,
	spatial::Spatial,
	startup_settings::StartupSettings,
};
use stardust_xr_molecules::{GrabData, Grabbable};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::{f32::consts::PI, sync::Arc};
use tween::{QuartInOut, Tweener};

fn model_from_icon(parent: &Spatial, icon: &Icon) -> Result<Model> {
	return match &icon.icon_type {
		IconType::Png => {
			let t = Transform::from_rotation_scale(
				Quat::from_rotation_x(PI / 2.0) * Quat::from_rotation_y(PI),
				[0.03, 0.03, 0.03],
			);

			let model = Model::create(
				parent,
				t,
				&ResourceID::new_namespaced("protostar", "hexagon/hexagon"),
			)?;
			model.set_material_parameter(
				1,
				"color",
				MaterialParameter::Color([0.0, 1.0, 1.0, 1.0]),
			)?;
			model.set_material_parameter(
				0,
				"diffuse",
				MaterialParameter::Texture(ResourceID::Direct(icon.path.clone())),
			)?;
			Ok(model)
		}
		IconType::Gltf => Ok(Model::create(
			parent,
			Transform::from_scale([0.05; 3]),
			&ResourceID::new_direct(icon.path.clone())?,
		)?),
		_ => panic!("Invalid Icon Type"),
	};
}

pub struct ProtoStar {
	client: Arc<Client>,
	position: Vector3<f32>,
	grabbable: Grabbable,
	field: BoxField,
	icon: Model,
	grabbable_shrink: Option<Tweener<f32, f64, QuartInOut>>,
	grabbable_grow: Option<Tweener<f32, f64, QuartInOut>>,
	execute_command: String,
	currently_shown: bool,
	grabbabe_move: Option<Tweener<f32, f64, QuartInOut>>,
}
impl ProtoStar {
	pub fn create_from_desktop_file(
		parent: &Spatial,
		position: impl Into<Vector3<f32>>,
		desktop_file: DesktopFile,
	) -> Result<Self> {
		// dbg!(&desktop_file);
		let raw_icons = desktop_file.get_raw_icons();
		let mut icon = raw_icons
			.clone()
			.into_iter()
			.find(|i| match i.icon_type {
				IconType::Gltf => true,
				_ => false,
			})
			.or(raw_icons.into_iter().max_by_key(|i| i.size));

		match icon {
			Some(i) => {
				icon = match i.cached_process(128) {
					Ok(i) => Some(i),
					_ => None,
				}
			}
			None => {}
		}

		Self::new_raw(
			parent,
			position,
			icon,
			desktop_file.command.ok_or_else(|| eyre!("No command"))?,
		)
	}
	pub fn new_raw(
		parent: &Spatial,
		position: impl Into<Vector3<f32>>,
		icon: Option<Icon>,
		execute_command: String,
	) -> Result<Self> {
		let position = position.into();
		let field = BoxField::create(
			parent,
			Transform::default(),
			match icon.as_ref() {
				Some(_) => [0.05, 0.0665, 0.005],
				_ => [0.05; 3],
			},
		)?;
		let grabbable = Grabbable::new(
			parent,
			Transform::from_position(position),
			&field,
			GrabData {
				max_distance: 0.01,
				..Default::default()
			},
		)?;
		field.set_spatial_parent(grabbable.content_parent())?;
		let icon = icon
			.map(|i| model_from_icon(grabbable.content_parent(), &i))
			.unwrap_or_else(|| {
				Ok(Model::create(
					grabbable.content_parent(),
					Transform::from_rotation_scale(
						Quat::from_xyzw(0.0, 0.707, 0.707, 0.0),
						[0.03, 0.03, 0.03],
					),
					&ResourceID::new_namespaced("protostar", "hexagon/hexagon"),
				)?)
			})?;
		Ok(ProtoStar {
			client: parent.client()?,
			position,
			grabbable,
			field,
			icon,
			grabbable_shrink: None,
			grabbable_grow: None,
			execute_command,
			currently_shown: true,
			grabbabe_move: None,
		})
	}
	pub fn content_parent(&self) -> &Spatial {
		self.grabbable.content_parent()
	}
	pub fn toggle(&mut self) {
		if self.currently_shown {
			self.grabbabe_move = Some(Tweener::quart_in_out(1.0, 0.0001, 0.25)); //TODO make the scale a parameter
		} else {
			self.grabbable
			.content_parent()
			.set_scale(None, Vector3::from([1.0; 3]))
			.unwrap();
			self.grabbabe_move = Some(Tweener::quart_in_out(0.0001, 1.0, 0.25));
		}
		self.currently_shown = !self.currently_shown;
	}
}
impl RootHandler for ProtoStar {
	fn frame(&mut self, info: FrameInfo) {

		self.grabbable.update(&info);

		if let Some(grabbabe_move) = &mut self.grabbabe_move {
			if !grabbabe_move.is_finished() {
				let scale = grabbabe_move.move_by(info.delta);
				self.grabbable
					.content_parent()
					.set_position(None, [self.position.x*scale, self.position.y*scale, self.position.z*scale])
					.unwrap();
			} else {
				if grabbabe_move.final_value() == 0.0001 {
					self.grabbable
					.content_parent()
					.set_scale(None, Vector3::from([0.001; 3]))
					.unwrap();
				}
				self.grabbabe_move = None;
			}
		}
		if let Some(grabbable_shrink) = &mut self.grabbable_shrink {
			if !grabbable_shrink.is_finished() {
				let scale = grabbable_shrink.move_by(info.delta);
				self.grabbable
					.content_parent()
					.set_scale(None, Vector3::from([scale; 3]))
					.unwrap();
			} else {
				if self.currently_shown {
					self.grabbable_grow = Some(Tweener::quart_in_out(0.0001, 1.0, 0.25)); //TODO make the scale a parameter
					self.grabbable.cancel_angular_velocity();
					self.grabbable.cancel_linear_velocity();
				}
				self.grabbable_shrink = None;
				self.grabbable
					.content_parent()
					.set_position(Some(self.client.get_root()), self.position)
					.unwrap();
				self.grabbable
					.content_parent()
					.set_rotation(Some(self.client.get_root()), Quat::default())
					.unwrap();
				self.icon
					.set_rotation(
						None,
						Quat::from_rotation_x(PI / 2.0) * Quat::from_rotation_y(PI),
					)
					.unwrap();
			}
		} else if let Some(grabbable_grow) = &mut self.grabbable_grow {
			if !grabbable_grow.is_finished() {
				let scale = grabbable_grow.move_by(info.delta);
				self.grabbable
					.content_parent()
					.set_scale(None, Vector3::from([scale; 3]))
					.unwrap();
			} else {
				self.grabbable_grow = None;
			}
		} else if self.grabbable.grab_action().actor_stopped() {
			let startup_settings = StartupSettings::create(&self.field.client().unwrap()).unwrap();
			startup_settings
				.set_root(self.grabbable.content_parent())
				.unwrap();
			self.grabbable_shrink = Some(Tweener::quart_in_out(0.03, 0.0001, 0.25)); //TODO make the scale a parameter
			let distance_future = self
				.grabbable
				.content_parent()
				.get_position_rotation_scale(self.client.get_root())
				.unwrap();

			let executable = dbg!(self.execute_command.clone());

			//TODO: split the executable string for  the args
			tokio::task::spawn(async move {
				let distance_vector = distance_future.await.ok().unwrap().0;
				let distance =
					distance_vector.x.abs() + distance_vector.y.abs() + distance_vector.z.abs();
				if dbg!(distance) > 1.0 {
					let future = startup_settings.generate_startup_token().unwrap();

					std::env::set_var("STARDUST_STARTUP_TOKEN", future.await.unwrap());

					unsafe {
						Command::new(executable)
							.stdin(Stdio::null())
							.stdout(Stdio::null())
							.stderr(Stdio::null())
							.pre_exec(|| {
								_ = setsid();
								Ok(())
							})
							.spawn()
							.expect("Failed to start child process");
					}
				}
			});
		}
	}
}
