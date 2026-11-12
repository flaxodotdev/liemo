// The production pipeline for a whole video made with the AI chat: seven stages, each leaving a
// document beside the project that the person can read, edit or overrule. Small changes skip it.

use anyhow::{bail, Result};
use std::path::PathBuf;

/// id, name, what the chat shows while it's under way, and what the document holds.
pub const STAGES: [(&str, &str, &str, &str); 7] = [
    ("analysis", "Analysis", "Analysing the material", "audience, the promise, format and length, the material there is and what's missing"),
    ("brand", "Brand", "Collecting the brand", "colours, type, logo and tone, from the brand kit, the footage or the request"),
    ("script", "Script", "Writing the script", "every shot in order: what's on screen, its message, its words, its timing"),
    ("motion", "Motion", "Defining the motion language", "which text animations, transitions, effects and overlays play which shot, and why"),
    ("build", "Build", "Building the video", "what was put on the timeline, and the frames rendered to check it"),
    ("choreography", "Choreography", "Choreographing the whole video", "rhythm, continuity between shots, reading time, the music's shape; design_check results"),
    ("review", "Review", "Reviewing the result", "every note, each closed with evidence (a frame, a check, a fix)"),
];

pub fn stage(id: &str) -> Option<usize> {
    STAGES.iter().position(|s| s.0 == id)
}

/// The project's documents folder.
pub fn dir(project: &str) -> PathBuf {
    crate::storage::root().join("docs").join(project)
}

pub fn path(project: &str, stage: &str) -> PathBuf {
    dir(project).join(format!("{stage}.md"))
}

pub fn write(project: &str, stage: &str, content: &str) -> Result<PathBuf> {
    if self::stage(stage).is_none() {
        bail!("Unknown stage {stage}: use one of {}", STAGES.map(|s| s.0).join(", "));
    }
    let p = path(project, stage);
    crate::storage::write_atomic(&p, content.as_bytes())?;
    Ok(p)
}

pub fn read(project: &str, stage: &str) -> Option<String> {
    std::fs::read_to_string(path(project, stage)).ok()
}

/// Which stages have their document, in order.
pub fn done(project: &str) -> Vec<bool> {
    STAGES.iter().map(|s| path(project, s.0).exists()).collect()
}

/// For the AI chat's instructions.
pub fn describe() -> String {
    let mut s = String::from(
        "When asked to make a whole video (not a small change), work through these stages in order. At each, write its \
         document with write_doc before moving on, so the user can read and overrule every decision; read_docs shows \
         what's already decided (the user may have edited it). Keep documents short and concrete.",
    );
    for (i, (id, name, _, what)) in STAGES.iter().enumerate() {
        s += &format!("\n{}. {name} ({id}): {what}.", i + 1);
    }
    s += "\nBefore calling a video done, run design_check and fix or justify every issue in review. For a single \
          pointed change, just make it and skip the stages.";
    s
}
