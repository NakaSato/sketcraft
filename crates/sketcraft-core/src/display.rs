use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FaceStyle {
    Wireframe,
    HiddenLine,
    Monochrome,
    Shaded,
    #[default]
    Materials,
}
impl FaceStyle {
    pub const ALL: [Self; 5] = [
        Self::Wireframe,
        Self::HiddenLine,
        Self::Monochrome,
        Self::Shaded,
        Self::Materials,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Wireframe => "Wireframe",
            Self::HiddenLine => "Hidden line",
            Self::Monochrome => "Monochrome",
            Self::Shaded => "Shaded",
            Self::Materials => "Shaded with materials",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplaySettings {
    pub joinery_preview: bool,
    /// Show clickable markers at saved wood joints.
    pub wood_joinery: bool,
    pub fasteners: bool,
    pub fastener_preview: bool,
    pub fastener_face_handles: bool,
    pub units: crate::UnitSettings,
    /// Smooth only cast-shadow boundaries, preserving original geometry rendering.
    #[serde(alias = "antialiasing")]
    pub shadow_antialiasing: bool,
    pub grid: bool,
    pub grid_auto_size: bool,
    pub grid_spacing: f64,
    pub grid_infinite: bool,
    /// Side length in meters of the finite square centered on the model origin.
    pub grid_max_length: f64,
    pub axis_colors: [[u8; 3]; 3],
    pub inference_color: [u8; 3],
    pub midpoint_color: [u8; 3],
    pub endpoint_color: [u8; 3],
    pub on_edge_color: [u8; 3],
    pub on_face_color: [u8; 3],
    pub axes: bool,
    pub guides: bool,
    pub dimensions: bool,
    pub hidden_geometry: bool,
    pub hidden_objects: bool,
    pub group_emphasis: bool,
    /// Show group names at their projected bounds centers.
    pub group_names: bool,
    pub face_style: FaceStyle,
    pub x_ray: bool,
    pub edges: bool,
    pub back_edges: bool,
    pub profiles: bool,
    pub profile_width: f64,
    pub depth_cue: bool,
    pub endpoints: bool,
    pub extensions: bool,
    pub color_by_tag: bool,
    pub section_planes: bool,
    pub section_cuts: bool,
    pub section_fill: bool,
    pub section_height: f64,
    pub section_axes: [bool; 3],
    pub section_offsets: [f64; 2],
    pub shadows: bool,
    pub shadow_darkness: f64,
    pub sun_angle: f64,
    pub sun_elevation: f64,
    /// Derive `sun_angle` and `sun_elevation` from the date, time and latitude below.
    pub sun_from_time: bool,
    /// Month (1–12) and day of the month for the solar position.
    pub sun_month: u8,
    pub sun_day: u8,
    /// Local solar time in minutes after midnight.
    pub sun_minutes: u16,
    /// Degrees north (positive) or south of the equator. The model's green axis points north.
    pub sun_latitude: f64,
    pub fog: bool,
    pub fog_start: f64,
    pub fog_end: f64,
    pub background: [u8; 3],
    pub sky: bool,
    pub ground: bool,
    pub sky_color: [u8; 3],
    pub ground_color: [u8; 3],
    pub horizon_color: [u8; 3],
}
impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            joinery_preview: true,
            wood_joinery: true,
            fasteners: true,
            fastener_preview: false,
            fastener_face_handles: false,
            units: crate::UnitSettings::default(),
            shadow_antialiasing: true,
            grid: true,
            grid_auto_size: true,
            grid_spacing: 1.,
            grid_infinite: true,
            grid_max_length: 100.,
            axis_colors: [[192, 94, 76], [97, 145, 92], [90, 133, 176]],
            inference_color: [97, 145, 92],
            midpoint_color: [85, 150, 183],
            endpoint_color: [46, 160, 67],
            on_edge_color: [214, 56, 56],
            on_face_color: [52, 110, 220],
            axes: true,
            guides: true,
            dimensions: true,
            hidden_geometry: false,
            hidden_objects: false,
            group_emphasis: true,
            group_names: false,
            face_style: FaceStyle::Materials,
            x_ray: false,
            edges: true,
            back_edges: false,
            profiles: false,
            profile_width: 3.,
            depth_cue: false,
            endpoints: false,
            extensions: false,
            color_by_tag: false,
            section_planes: false,
            section_cuts: false,
            section_fill: true,
            section_height: 1.,
            section_axes: [false, false, true],
            section_offsets: [1., 1.],
            shadows: false,
            shadow_darkness: 0.35,
            sun_angle: 135.,
            sun_elevation: 45.,
            sun_from_time: false,
            sun_month: 6,
            sun_day: 21,
            sun_minutes: 15 * 60,
            sun_latitude: 40.,
            fog: false,
            fog_start: 10.,
            fog_end: 50.,
            background: [240, 242, 234],
            sky: true,
            ground: true,
            sky_color: [153, 190, 215],
            ground_color: [218, 214, 190],
            horizon_color: [238, 240, 226],
        }
    }
}
/// Days in `month` (1–12) of a non-leap year; the solar position ignores leap days.
pub fn days_in_month(month: u8) -> u8 {
    match month {
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Where the sun is for a date, local solar time and latitude, as
/// `(compass azimuth clockwise from north, elevation above the horizon)` in degrees.
/// Uses the standard declination and hour-angle approximation, accurate to about a degree.
pub fn solar_position(month: u8, day: u8, minutes: u16, latitude: f64) -> (f64, f64) {
    let month = month.clamp(1, 12);
    let day_of_year: u32 = (1..month).map(|m| u32::from(days_in_month(m))).sum::<u32>()
        + u32::from(day.clamp(1, days_in_month(month)));
    let declination = 23.44f64.to_radians()
        * (std::f64::consts::TAU * (284. + f64::from(day_of_year)) / 365.).sin();
    let hour_angle = (f64::from(minutes.min(24 * 60 - 1)) / 60. - 12.) * 15f64.to_radians();
    let latitude = latitude.clamp(-90., 90.).to_radians();
    let elevation = (latitude.sin() * declination.sin()
        + latitude.cos() * declination.cos() * hour_angle.cos())
    .clamp(-1., 1.)
    .asin();
    let azimuth = (-declination.cos() * hour_angle.sin()).atan2(
        declination.sin() * latitude.cos() - declination.cos() * latitude.sin() * hour_angle.cos(),
    );
    (azimuth.to_degrees().rem_euclid(360.), elevation.to_degrees())
}

impl DisplaySettings {
    /// The sun's true elevation for the saved date, time and latitude; negative at night.
    pub fn solar_elevation(&self) -> f64 {
        solar_position(self.sun_month, self.sun_day, self.sun_minutes, self.sun_latitude).1
    }
    /// With `sun_from_time` on, point the sun from the date, time and latitude. The model's
    /// green (+Y) axis is north and red (+X) is east. Elevation stays within the renderer's
    /// 5°–85° range, so a sun below the horizon casts the longest shadows instead of none.
    pub fn apply_solar_time(&mut self) {
        if !self.sun_from_time {
            return;
        }
        self.sun_day = self.sun_day.clamp(1, days_in_month(self.sun_month.clamp(1, 12)));
        let (azimuth, elevation) =
            solar_position(self.sun_month, self.sun_day, self.sun_minutes, self.sun_latitude);
        self.sun_angle = (90. - azimuth).rem_euclid(360.);
        self.sun_elevation = elevation.clamp(5., 85.);
    }
    /// Z retains the legacy height field so older documents keep their cut.
    pub fn section_limits(&self) -> [Option<f64>; 3] {
        let offsets = [
            self.section_offsets[0],
            self.section_offsets[1],
            self.section_height,
        ];
        std::array::from_fn(|axis| {
            (self.section_cuts && self.section_axes[axis]).then_some(offsets[axis])
        })
    }
    pub fn section_contains(&self, point: crate::DVec3) -> bool {
        self.section_limits()
            .iter()
            .enumerate()
            .all(|(axis, limit)| limit.is_none_or(|v| point[axis] <= v + 1e-8))
    }
    pub fn section_segment(
        &self,
        mut a: crate::DVec3,
        mut b: crate::DVec3,
    ) -> Option<(crate::DVec3, crate::DVec3)> {
        for (axis, limit) in self.section_limits().into_iter().enumerate() {
            let Some(limit) = limit else { continue };
            if a[axis] > limit && b[axis] > limit {
                return None;
            }
            if (a[axis] > limit) != (b[axis] > limit) {
                let mut cut = a.lerp(b, (limit - a[axis]) / (b[axis] - a[axis]));
                cut[axis] = limit;
                if a[axis] > limit {
                    a = cut;
                } else {
                    b = cut;
                }
            }
        }
        Some((a, b))
    }
    /// Clip a triangle into at most six vertices without per-frame allocations.
    pub fn section_triangle(&self, triangle: [crate::DVec3; 3]) -> ([crate::DVec3; 6], usize) {
        let mut polygon = [crate::DVec3::ZERO; 6];
        polygon[..3].copy_from_slice(&triangle);
        let mut len = 3;
        for (axis, limit) in self.section_limits().into_iter().enumerate() {
            let Some(limit) = limit else { continue };
            let input = polygon;
            let input_len = len;
            len = 0;
            for i in 0..input_len {
                let a = input[i];
                let b = input[(i + 1) % input_len];
                if a[axis] <= limit {
                    polygon[len] = a;
                    len += 1;
                }
                if (a[axis] <= limit) != (b[axis] <= limit) {
                    let mut cut = a.lerp(b, (limit - a[axis]) / (b[axis] - a[axis]));
                    cut[axis] = limit;
                    polygon[len] = cut;
                    len += 1;
                }
            }
        }
        (polygon, len)
    }
    pub fn valid(&self) -> bool {
        (0.000001..=1e6).contains(&self.grid_spacing)
            && (0.000001..=1e6).contains(&self.grid_max_length)
            && (1. ..=8.).contains(&self.profile_width)
            && self
                .section_offsets
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 1e10)
            && self.section_height.is_finite()
            && self.section_height.abs() <= 1e10
            && (0. ..=1.).contains(&self.shadow_darkness)
            && (0. ..=360.).contains(&self.sun_angle)
            && (5. ..=85.).contains(&self.sun_elevation)
            && (1..=12).contains(&self.sun_month)
            && (1..=days_in_month(self.sun_month)).contains(&self.sun_day)
            && self.sun_minutes < 24 * 60
            && (-90. ..=90.).contains(&self.sun_latitude)
            && self.fog_start.is_finite()
            && self.fog_start >= 0.
            && self.fog_end.is_finite()
            && self.fog_end > self.fog_start
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    #[test]
    fn solar_position_matches_known_noon_elevations_and_directions() {
        // Noon elevation at 40°N is 90° − 40° + declination.
        for (month, day, expected) in [(6, 21, 73.44), (3, 21, 50.), (12, 21, 26.56)] {
            let (azimuth, elevation) = solar_position(month, day, 12 * 60, 40.);
            assert!((elevation - expected).abs() < 1., "{month}/{day}: {elevation}");
            assert!((azimuth - 180.).abs() < 1e-6, "noon sun is due south: {azimuth}");
        }
        // Equinox sunrise at the equator is due east on the horizon.
        let (azimuth, elevation) = solar_position(3, 21, 6 * 60, 0.);
        assert!(elevation.abs() < 1.5 && (azimuth - 90.).abs() < 1.5, "{azimuth} {elevation}");
        // Southern hemisphere noon sun is due north.
        assert!(solar_position(12, 21, 12 * 60, -35.).0.abs() < 1e-6);
        // Morning in the east, afternoon in the west.
        assert!(solar_position(6, 21, 9 * 60, 40.).0 < 180.);
        assert!(solar_position(6, 21, 15 * 60, 40.).0 > 180.);
    }

    #[test]
    fn solar_time_points_the_light_with_green_north_and_clamps_night() {
        let mut d = DisplaySettings { sun_from_time: true, ..Default::default() };
        d.apply_solar_time(); // June 21, 15:00 at 40°N: high in the south-west.
        let a = d.sun_angle.to_radians();
        assert!(a.cos() < 0. && a.sin() < 0., "light comes from the south-west: {}", d.sun_angle);
        assert!(d.sun_elevation > 40. && d.sun_elevation < 60.);
        assert!(d.valid());

        d.sun_minutes = 0;
        d.apply_solar_time();
        assert!(d.solar_elevation() < 0.);
        assert_eq!(d.sun_elevation, 5.);
        assert!(d.valid());

        // Manual mode leaves the saved angles alone.
        let mut manual = DisplaySettings { sun_angle: 10., sun_elevation: 20., ..Default::default() };
        manual.apply_solar_time();
        assert_eq!((manual.sun_angle, manual.sun_elevation), (10., 20.));
    }

    #[test]
    fn older_display_settings_load_with_the_manual_sun() {
        let json = serde_json::json!({ "shadows": true, "sun_angle": 30., "sun_elevation": 25. });
        let d: DisplaySettings = serde_json::from_value(json).unwrap();
        assert!(!d.sun_from_time);
        assert_eq!((d.sun_angle, d.sun_elevation), (30., 25.));
        assert_eq!((d.sun_month, d.sun_day, d.sun_minutes), (6, 21, 900));
        assert!(d.valid());
    }

    #[test]
    fn grid_and_wood_joinery_default_on_and_preserve_saved_visibility() {
        for display in [
            DisplaySettings::default(),
            serde_json::from_str::<DisplaySettings>("{}").unwrap(),
        ] {
            assert!(display.grid);
            assert!(display.wood_joinery);
        }
        let saved: DisplaySettings =
            serde_json::from_str(r#"{"grid":false,"wood_joinery":false}"#).unwrap();
        assert!(!saved.grid);
        assert!(!saved.wood_joinery);
    }

    #[test]
    fn finite_grid_defaults_round_trip_and_validate_length() {
        let old: DisplaySettings =
            serde_json::from_str(r#"{"grid":true,"grid_spacing":0.5}"#).unwrap();
        assert!(old.grid_infinite);
        assert_eq!(old.grid_max_length, 100.);
        let finite = DisplaySettings {
            grid_infinite: false,
            grid_max_length: 30.48,
            ..old
        };
        assert!(finite.valid());
        assert_eq!(
            serde_json::from_str::<DisplaySettings>(&serde_json::to_string(&finite).unwrap())
                .unwrap(),
            finite
        );
        for length in [0., -1., f64::NAN, f64::INFINITY, 1e7] {
            assert!(
                !DisplaySettings {
                    grid_max_length: length,
                    ..finite.clone()
                }
                .valid()
            );
        }
    }

    #[test]
    fn sections_restore_legacy_z_and_round_trip_all_axes() {
        let old: DisplaySettings =
            serde_json::from_str(r#"{"section_cuts":true,"section_height":2.5}"#).unwrap();
        assert_eq!(old.section_limits(), [None, None, Some(2.5)]);
        let mut display = old;
        display.section_axes = [true; 3];
        display.section_offsets = [-2., 4.];
        let restored: DisplaySettings =
            serde_json::from_str(&serde_json::to_string(&display).unwrap()).unwrap();
        assert_eq!(restored, display);
        assert_eq!(restored.section_limits(), [Some(-2.), Some(4.), Some(2.5)]);
        for value in [f64::NAN, f64::INFINITY, 1e11] {
            display.section_offsets[0] = value;
            assert!(!display.valid());
        }
    }

    #[test]
    fn sections_clip_segments_and_triangles_against_every_active_axis() {
        use crate::DVec3;
        let display = DisplaySettings {
            section_cuts: true,
            section_axes: [true; 3],
            ..Default::default()
        };
        let (a, b) = display
            .section_segment(DVec3::ZERO, DVec3::splat(2.))
            .unwrap();
        assert_eq!(a, DVec3::ZERO);
        assert_eq!(b, DVec3::ONE);
        assert!(
            display
                .section_segment(DVec3::splat(2.), DVec3::splat(3.))
                .is_none()
        );
        let (polygon, len) =
            display.section_triangle([DVec3::ZERO, DVec3::new(3., 0., 2.), DVec3::new(0., 3., 2.)]);
        assert!(len >= 3);
        assert!(polygon[..len].iter().all(|&p| display.section_contains(p)));
        for axis in 0..3 {
            let mut display = display.clone();
            display.section_axes = [false; 3];
            display.section_axes[axis] = true;
            assert!(!display.section_contains(DVec3::splat(2.)));
            assert!(display.section_contains(DVec3::ZERO));
        }
    }

    #[test]
    fn older_display_settings_default_grid_and_colors_and_reject_invalid_spacing() {
        let display: DisplaySettings = serde_json::from_str("{}").unwrap();
        assert_eq!(display, DisplaySettings::default());
        assert!(display.grid_auto_size);
        for spacing in [0., -1., f64::NAN, f64::INFINITY, 1e7] {
            assert!(
                !DisplaySettings {
                    grid_spacing: spacing,
                    ..display.clone()
                }
                .valid()
            );
        }
    }

    #[test]
    fn display_settings_round_trip_default_old_documents_and_validate_ranges() {
        let mut doc = Document::default();
        doc.display.hidden_objects = true;
        doc.display.axes = false;
        doc.display.fog = true;
        doc.display.face_style = FaceStyle::Wireframe;
        assert_eq!(
            Document::from_json(&serde_json::to_string(&doc).unwrap())
                .unwrap()
                .display,
            doc.display
        );
        let mut old = serde_json::to_value(&doc).unwrap();
        old.as_object_mut().unwrap().remove("display");
        assert_eq!(
            Document::from_json(&old.to_string()).unwrap().display,
            DisplaySettings::default()
        );
        doc.display.fog_end = doc.display.fog_start;
        assert!(Document::from_json(&serde_json::to_string(&doc).unwrap()).is_err());
    }
}
