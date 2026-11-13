//! Colour management: declaring what a surface's pixels mean.
//!
//! An image description is only valid after its `done` event, so callers must round-trip
//! before attaching it; attaching early is silently ignored by the compositor.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::wp::color_management::v1::client::{
    wp_color_management_output_v1::WpColorManagementOutputV1,
    wp_color_management_surface_feedback_v1::WpColorManagementSurfaceFeedbackV1,
    wp_color_management_surface_v1::WpColorManagementSurfaceV1,
    wp_color_manager_v1::{self, WpColorManagerV1},
    wp_image_description_creator_params_v1::WpImageDescriptionCreatorParamsV1,
    wp_image_description_info_v1::{self, WpImageDescriptionInfoV1},
    wp_image_description_v1::{self, WpImageDescriptionV1},
};

use crate::window::WindowState;

impl Dispatch<wayland_client::protocol::wl_surface::WlSurface, ()> for WindowState {
    fn event(
        state: &mut Self,
        _: &wayland_client::protocol::wl_surface::WlSurface,
        event: <wayland_client::protocol::wl_surface::WlSurface as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use wayland_client::protocol::wl_surface::Event;
        match event {
            // The surface may be on several outputs; the last `enter` is the one it is drawn
            // on, and a `leave` of that output drops back to the one named at creation.
            Event::Enter { output } => {
                state.entered_output = Some(output);
                state.colour_dirty = true;
            }
            Event::Leave { output } => {
                if state
                    .entered_output
                    .as_ref()
                    .is_some_and(|current| current.id() == output.id())
                {
                    state.entered_output = None;
                }
                state.colour_dirty = true;
            }
            _ => {}
        }
    }
}

impl Dispatch<WpColorManagementOutputV1, ()> for WindowState {
    fn event(
        state: &mut Self,
        _: &WpColorManagementOutputV1,
        event: <WpColorManagementOutputV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // The panel's description changed; the surface has to be resolved again.
        if matches!(
            event,
            wayland_protocols::wp::color_management::v1::client::wp_color_management_output_v1::Event::ImageDescriptionChanged
        ) {
            state.colour_dirty = true;
        }
    }
}

impl Dispatch<WpColorManagerV1, ()> for WindowState {
    fn event(
        state: &mut Self,
        _: &WpColorManagerV1,
        event: wp_color_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use wp_color_manager_v1::Event;
        match event {
            // Features gate the creator requests: using an unadvertised one is a
            // protocol error, not a graceful failure.
            Event::SupportedFeature { feature } => {
                if let Ok(f) = feature.into_result() {
                    state.colour_features |= 1 << (f as u32);
                }
            }
            Event::Done => state.colour_features_known = true,
            _ => {}
        }
    }
}

impl Dispatch<WpColorManagementSurfaceFeedbackV1, ()> for WindowState {
    fn event(
        state: &mut Self,
        _: &WpColorManagementSurfaceFeedbackV1,
        event: <WpColorManagementSurfaceFeedbackV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(
            event,
            wayland_protocols::wp::color_management::v1::client::wp_color_management_surface_feedback_v1::Event::PreferredChanged { .. }
        ) {
            state.colour_dirty = true;
        }
    }
}

impl Dispatch<WpColorManagementSurfaceV1, ()> for WindowState {
    fn event(
        _: &mut Self,
        _: &WpColorManagementSurfaceV1,
        _: <WpColorManagementSurfaceV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpImageDescriptionCreatorParamsV1, ()> for WindowState {
    fn event(
        _: &mut Self,
        _: &WpImageDescriptionCreatorParamsV1,
        _: <WpImageDescriptionCreatorParamsV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpImageDescriptionInfoV1, ()> for WindowState {
    fn event(
        state: &mut Self,
        _: &WpImageDescriptionInfoV1,
        event: wp_image_description_info_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // The reference white is the anchor; the range tells us what the display can show.
        match event {
            wp_image_description_info_v1::Event::Luminances {
                min_lum,
                max_lum,
                reference_lum,
            } => {
                state.preferred_luminances = Some((min_lum, max_lum, reference_lum));
            }
            // The compositor's own tf and primaries: what it expects our buffer to be.
            wp_image_description_info_v1::Event::TfNamed { tf } => {
                state.preferred_tf = tf.into_result().ok().map(|v| v as u32);
            }
            wp_image_description_info_v1::Event::PrimariesNamed { primaries } => {
                state.preferred_primaries = primaries.into_result().ok().map(|v| v as u32);
            }
            wp_image_description_info_v1::Event::Primaries {
                r_x,
                r_y,
                g_x,
                g_y,
                b_x,
                b_y,
                w_x,
                w_y,
            } => {
                // Each coordinate arrives multiplied by one million.
                let s = 1.0 / 1_000_000.0;
                state.preferred_chromaticities = Some(ui_core::colour::Chromaticities {
                    red: (r_x as f32 * s, r_y as f32 * s),
                    green: (g_x as f32 * s, g_y as f32 * s),
                    blue: (b_x as f32 * s, b_y as f32 * s),
                    white: (w_x as f32 * s, w_y as f32 * s),
                });
            }
            _ => {}
        }
    }
}

impl Dispatch<WpImageDescriptionV1, ()> for WindowState {
    fn event(
        state: &mut Self,
        desc: &WpImageDescriptionV1,
        event: wp_image_description_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        // A rejected description is silent otherwise: the surface keeps its old meaning.
        match event {
            wp_image_description_v1::Event::Failed { cause, .. } => {
                state.colour_failed = Some(cause.into_result().map(|c| c as u32).unwrap_or(0));
            }
            // From interface v2 the compositor sends `ready2` instead of `ready`.
            wp_image_description_v1::Event::Ready2 { .. }
            | wp_image_description_v1::Event::Ready { .. }
                // Whether this is allowed depends on how the description was made: the
                // preferred one is rejected with `no_information` on this compositor.
                if state.allow_info => {
                    desc.get_information(qh, ());
                }
            _ => {}
        }
    }
}
