use quote::{format_ident, quote};
use rmk_config::EncoderPhase;
use rmk_config::resolved::hardware::{ChipModel, EncoderConfig, EncoderResolution};

use super::Initializer;
use crate::codegen::chip::gpio::convert_gpio_str_to_input_pin;

fn resolve_encoder_resolution(config: &EncoderResolution) -> Result<u8, &'static str> {
    match config {
        EncoderResolution::Value(0) => Err("resolution must be at least 1"),
        EncoderResolution::Value(resolution) => Ok(*resolution),
        EncoderResolution::Derived { detent: 0, .. } => Err("resolution detent must be at least 1"),
        EncoderResolution::Derived { detent, pulse } => {
            let resolution = u16::from(*pulse) * 4 / u16::from(*detent);
            if resolution == 0 {
                return Err("derived resolution must be at least 1");
            }
            u8::try_from(resolution).map_err(|_| "derived resolution must not exceed 255")
        }
    }
}

/// Expand encoder device, this function returns the (device_initializer, processor_initializer)
///
/// `id_offset` is the offset of the encoder id, it is used to distinguish the encoder id between central and peripheral
pub(crate) fn expand_encoder_device(
    id_offset: usize,
    encoder_config: Vec<EncoderConfig>,
    chip: &ChipModel,
) -> (Vec<Initializer>, Vec<Initializer>) {
    if encoder_config.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut device_initializer = vec![];

    // Create rotary encoders
    for (idx, encoder) in encoder_config.iter().enumerate() {
        let encoder_id = idx as u8 + id_offset as u8;

        let pull = if encoder.internal_pullup {
            Some(true)
        } else {
            None
        };

        // Initialize pins
        let pin_a = convert_gpio_str_to_input_pin(chip, encoder.pin_a.clone(), false, pull);
        let pin_b = convert_gpio_str_to_input_pin(chip, encoder.pin_b.clone(), false, pull);

        let encoder_name = format_ident!("encoder_{}", encoder_id);
        // encoder_names.push(encoder_name.clone());

        let debounce_chain = match encoder.debounce_ms {
            Some(ms) if ms > 0 => quote! { .with_debounce(#ms) },
            _ => quote! {},
        };

        // Create different types of encoders based on the phase field
        let encoder_device = match encoder.phase {
            EncoderPhase::E8h7 => {
                quote! {
                    let mut #encoder_name = ::rmk::input_device::rotary_encoder::RotaryEncoder::with_phase(
                        #pin_a,
                        #pin_b,
                        ::rmk::input_device::rotary_encoder::E8H7Phase,
                        #encoder_id
                    )#debounce_chain;
                }
            }
            EncoderPhase::Resolution => {
                // When phase is "resolution", ensure resolution and reverse are set
                let resolution = encoder
                    .resolution
                    .as_ref()
                    .ok_or("resolution must be set when phase is 'resolution'")
                    .and_then(resolve_encoder_resolution)
                    .unwrap_or_else(|message| panic!("encoder {encoder_id}: {message}"));
                let reverse = encoder.reverse.unwrap_or(false);

                quote! {
                    let mut #encoder_name = ::rmk::input_device::rotary_encoder::RotaryEncoder::with_resolution(
                        #pin_a,
                        #pin_b,
                        #resolution,
                        #reverse,
                        #encoder_id
                    )#debounce_chain;
                }
            }
            EncoderPhase::Default => {
                quote! {
                    let mut #encoder_name = ::rmk::input_device::rotary_encoder::RotaryEncoder::with_phase(
                        #pin_a,
                        #pin_b,
                        ::rmk::input_device::rotary_encoder::DefaultPhase,
                        #encoder_id
                    )#debounce_chain;
                }
            }
        };

        device_initializer.push(Initializer {
            initializer: encoder_device,
            var_name: encoder_name,
        });
    }

    (device_initializer, vec![])
}

#[cfg(test)]
mod tests {
    use super::resolve_encoder_resolution;
    use rmk_config::resolved::hardware::EncoderResolution;

    #[test]
    fn derived_resolution_uses_wide_arithmetic() {
        assert_eq!(
            resolve_encoder_resolution(&EncoderResolution::Derived {
                detent: 200,
                pulse: 100,
            }),
            Ok(2)
        );
    }

    #[test]
    fn zero_and_out_of_range_resolutions_are_rejected() {
        assert!(resolve_encoder_resolution(&EncoderResolution::Value(0)).is_err());
        assert!(
            resolve_encoder_resolution(&EncoderResolution::Derived {
                detent: 0,
                pulse: 1
            })
            .is_err()
        );
        assert!(
            resolve_encoder_resolution(&EncoderResolution::Derived {
                detent: 1,
                pulse: 100
            })
            .is_err()
        );
        assert!(
            resolve_encoder_resolution(&EncoderResolution::Derived {
                detent: 200,
                pulse: 1
            })
            .is_err()
        );
    }
}
