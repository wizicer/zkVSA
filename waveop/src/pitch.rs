use clap::ValueEnum;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum PitchAlgo {
    OLA,
    #[value(alias = "simple")]
    RESAMPLE,
    #[value(name = "td-psola", alias = "tdpsola", alias = "psola")]
    TDPSOLA,
    #[value(name = "pvtsm-f32", alias = "pv-tsm-f32", alias = "pv-tsm")]
    PvtsmF32,
    #[value(name = "pvtsm-m32", alias = "pv-tsm-m32")]
    PvtsmM32,
    #[value(name = "pvtsm-f377", alias = "pv-tsm-f377", alias = "f377")]
    PvtsmF377,
}

// Submodules with individual algorithm implementations
mod ola;
mod pvtsm;
mod resample;
mod tdpsola;
pub mod utils;

// Algorithms are implemented in submodules; this file only routes calls.

pub fn pitch_shift_with_algo(samples: &[i16], semitones: f32, algo: PitchAlgo, cbor_dir: Option<&std::path::Path>, use_half_up: bool) -> Vec<i16> {
    match algo {
        PitchAlgo::OLA => ola::pitch_shift_ola(samples, semitones),
        PitchAlgo::RESAMPLE => resample::pitch_shift_resample(samples, semitones),
        PitchAlgo::TDPSOLA => tdpsola::pitch_shift_td_psola(samples, semitones),
        PitchAlgo::PvtsmF32 => pvtsm::pitch_shift_pv_tsm(samples, semitones, pvtsm::PvAccum::F32, None, use_half_up),
        PitchAlgo::PvtsmM32 => pvtsm::pitch_shift_pv_tsm(samples, semitones, pvtsm::PvAccum::M32, None, use_half_up),
        PitchAlgo::PvtsmF377 => pvtsm::pitch_shift_pv_tsm(samples, semitones, pvtsm::PvAccum::F377, cbor_dir, use_half_up),
    }
}
