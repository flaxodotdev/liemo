# Changelog

What's new in each Liemo release, on the Mac, Windows, Linux, iPhone and iPad.

## 0.10.0

### New
- **A music player on iPhone and iPad**: tap ♪ on the projects screen to switch Liemo into a
  music player (and Editor to switch back). It starts empty: add songs, music videos or whole
  folders from Files, or drop them into Files › On My iPhone › Liemo › Music. Browse by song,
  artist, album, genre, playlist and video; search; queue, shuffle, repeat, a sleep timer, the
  lock screen and Control Center, and it keeps playing with the screen off.
- **Rework any song as it plays**: speed, pitch (with or without following the speed, like a
  record), reverb and room, bass and treble, with presets like Sped up, Nightcore, Slowed +
  reverb and Lo-fi. Liemo can remember each song's own sound.
- **Loop the hook**: Liemo finds the part of a song that hits and loops it seamlessly. Pick it
  again on the song's waveform, set its length in bars, or let Liemo find it.
- **Make a reel from your music**: any song in the player opens Quick reel with that song ready,
  lyrics included.
- **Liemo for Android** (early): projects, the editor with its timeline, preview and inspector,
  the library, Quick reel and export, on the same engine as every other Liemo.

### Fixed
- On iPhone and iPad, resizing or trimming a clip no longer fails with “Missing "project"”, and
  dragging a clip's edge no longer jiggles.
- Projects keep their pictures, music and sound effects after an app update (iOS moves an app's
  folder when it updates; Liemo now finds the files again).
- Captions draw every letter: when a caption's font lacks one, iPhone and Android fall back on
  the system's fonts instead of a box. Invisible characters from pasted lyrics are ignored.
- Clips and captions laid end to end (Quick reel, the MCP tools) no longer overlap by a sliver
  and stack onto extra rows.

## 0.9.1

### New
- **As many overlays as you like**: drop an overlay where another one already sits and Liemo
  puts it on a new overlay track above, so layers stack without pushing each other along.
- **Text anywhere in the stack**: titles can go on any overlay track and tracks can be moved in
  front of or behind each other, so text can sit between two overlays. On iPhone and iPad, press
  and hold a track and choose Move up (in front) or Move down (behind).

### Better
- On iPhone and iPad, the timeline scrolls properly with many layers: overlapping main-video
  clips no longer hang off the bottom, and swiping up and down works even over a selected clip.

## 0.9.0

### New
- **Trim tools, like Resolve's**: the timeline has a Select · Trim · Blade switcher (A, T, B).
  With Trim, drag an edge to **ripple** (the rest of the track follows), ⌥-drag a cut to
  **roll** it, drag a clip to **slip** its footage in place, or ⌥-drag it to **slide** it
  between its neighbours. Blade cuts a clip wherever you click. On iPhone and iPad, pick
  Ripple, Roll, Slip or Slide from the new Trim button when a clip is selected.
- **Font swap**: a text animation that flicks through fonts many times a second.
- **Word inside**: put the same word again, smaller and in its own font and colour, in the
  middle of big text. There's a preset for it, and for Font swap.
- **Keyframes on iPhone and iPad**: a diamond beside position, scale, rotation, opacity, volume,
  effect strength and text settings adds a keyframe at the playhead.
- **More to animate**: text size, height, 3D tilt, outline width and the inside word's size
  now take keyframes.

### Better
- A titles track can now move between video tracks, so text can sit behind a cut-out subject.
  Right-click (or press and hold on iPhone and iPad) a track and choose Move down (behind).

## 0.8.0

### New
- **Earshot sound bank**: 252 sound effects made in code ship with Liemo. Search them in the SFX
  panel and drop them on the timeline. No download or account needed.
- **Dither text**: a new Stylize effect that draws the picture out of your own words. You can
  set the letter size, dither, contrast, shadow letters, scramble, ink and background, and
  invert it.
- **Pick the subject for Remove background**: click one object in the preview and only that one
  is kept, not every subject in the frame. Click again any time to pick a different one.
- **Glass text** on the glyphs themselves, for titles and captions.
- **Captions**: an invert option keeps them readable on any footage, and you can now set where
  they sit and how big they are.

### Better
- Resizing and trimming clips no longer hold up the editor.
- Zooming the timeline no longer blanks the filmstrip.
- Effects with text settings show up as text fields over MCP too.

## 0.7.0

- Song directory with lyrics, grid flash transition and liquid glass text.
