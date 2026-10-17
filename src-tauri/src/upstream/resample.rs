// Adapted from Meetily Community Edition (MIT).
use anyhow::Result;
use log::debug;
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
pub fn resample(input: &[f32], from_sample_rate: u32, to_sample_rate: u32) -> Result<Vec<f32>> {
    if input.is_empty() {
        return Ok(Vec::new());
    }

    // Fast path: No resampling needed
    if from_sample_rate == to_sample_rate {
        return Ok(input.to_vec());
    }

    let ratio = to_sample_rate as f64 / from_sample_rate as f64;

    // Adaptive parameters based on sample rate ratio
    let (sinc_len, interpolation_type, oversampling) = if ratio >= 2.0 {
        // Large upsampling (e.g., 8kHz → 16kHz, 16kHz → 48kHz, 24kHz → 48kHz)
        // Needs high quality to avoid artifacts
        debug!(
            "High-quality upsampling: {}Hz → {}Hz (ratio: {:.2}x)",
            from_sample_rate, to_sample_rate, ratio
        );
        (
            512,                          // Longer sinc for smoother interpolation
            SincInterpolationType::Cubic, // Cubic for best quality
            512,                          // Higher oversampling
        )
    } else if ratio >= 1.5 {
        // Moderate upsampling (e.g., 32kHz → 48kHz)
        debug!(
            "Moderate upsampling: {}Hz → {}Hz (ratio: {:.2}x)",
            from_sample_rate, to_sample_rate, ratio
        );
        (384, SincInterpolationType::Cubic, 384)
    } else if ratio > 1.0 {
        // Small upsampling (e.g., 44.1kHz → 48kHz)
        debug!(
            "Small upsampling: {}Hz → {}Hz (ratio: {:.2}x)",
            from_sample_rate, to_sample_rate, ratio
        );
        (256, SincInterpolationType::Linear, 256)
    } else if ratio <= 0.5 {
        // Large downsampling (e.g., 48kHz → 16kHz, 48kHz → 8kHz)
        // Needs strong anti-aliasing
        debug!(
            "Anti-aliased downsampling: {}Hz → {}Hz (ratio: {:.2}x)",
            from_sample_rate, to_sample_rate, ratio
        );
        (
            512,                          // Longer sinc for anti-aliasing
            SincInterpolationType::Cubic, // Cubic for quality
            512,
        )
    } else {
        // Moderate downsampling (e.g., 48kHz → 24kHz, 48kHz → 32kHz)
        debug!(
            "Moderate downsampling: {}Hz → {}Hz (ratio: {:.2}x)",
            from_sample_rate, to_sample_rate, ratio
        );
        (384, SincInterpolationType::Linear, 384)
    };

    let params = SincInterpolationParameters {
        sinc_len,
        f_cutoff: 0.95, // Preserve most of the frequency content
        interpolation: interpolation_type,
        oversampling_factor: oversampling,
        window: WindowFunction::BlackmanHarris2, // Best window for audio
    };

    let mut resampler = SincFixedIn::<f32>::new(
        ratio,
        2.0, // Maximum relative deviation
        params,
        input.len(),
        1, // Mono
    )?;

    let waves_in = vec![input.to_vec()];
    let waves_out = resampler.process(&waves_in, None)?;

    debug!(
        "Resampling complete: {} samples → {} samples",
        input.len(),
        waves_out[0].len()
    );

    Ok(waves_out.into_iter().next().unwrap())
}
