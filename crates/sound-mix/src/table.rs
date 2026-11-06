// SPDX-License-Identifier: MIT
//! What differs by file, and the step's names, so a test counts them.

/// One file's row, kept in the game's rows crate.
pub struct Row {
    /// The file a line speaks for.
    pub file: &'static str,
    /// The module its slots name, as a line's `c` writes it.
    pub module: &'static str,
}

/// The names this crate answers.
pub const FRONTS: [&str; 29] = [
    "AIL_startup",
    "AIL_shutdown",
    "AIL_waveOutOpen",
    "AIL_serve",
    "AIL_waveOutClose",
    "AIL_set_preference",
    "AIL_allocate_sample_handle",
    "AIL_init_sample",
    "AIL_release_sample_handle",
    "AIL_set_sample_volume",
    "AIL_set_sample_loop_count",
    "AIL_start_sample",
    "AIL_stop_sample",
    "AIL_end_sample",
    "AIL_set_sample_file",
    "AIL_sample_status",
    "AIL_allocate_file_sample",
    "AIL_set_sample_type",
    "AIL_set_sample_playback_rate",
    "AIL_set_sample_pan",
    "AIL_resume_sample",
    "AIL_set_sample_user_data",
    "AIL_sample_user_data",
    "AIL_register_EOS_callback",
    "AIL_load_sample_buffer",
    "AIL_sample_buffer_ready",
    "FILE_read",
    "MEM_free_lock",
    "AIL_midiOutOpen",
];

/// The names read as unreached: each ends the program by name, never a quiet answer.
pub const STOPS: [&str; 15] = [
    "AIL_active_sample_count",
    "AIL_minimum_sample_buffer_size",
    "AIL_lock_channel",
    "AIL_release_channel",
    "AIL_send_channel_voice_message",
    "AIL_allocate_sequence_handle",
    "AIL_init_sequence",
    "AIL_start_sequence",
    "AIL_stop_sequence",
    "AIL_resume_sequence",
    "AIL_sequence_status",
    "AIL_set_sequence_volume",
    "AIL_set_sequence_loop_count",
    "AIL_release_sequence_handle",
    "AIL_active_sequence_count",
];
