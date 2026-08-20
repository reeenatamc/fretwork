# fretwork

*[Léeme en español](README.es.md)*

Desktop app for capturing, storing, printing and rewriting guitar tabs.

## Why it exists

A lot of the songs I want to play are not tabbed anywhere on the internet. I
work them out by hand from YouTube: pause, rewind, slow down, write, repeat.
That work takes hours and until now it evaporated. It was not in a Drive, not in
a repo, not anywhere I could get it back from.

There are tab editors that solve part of this. I could have used one. But I
would rather build my own tool than settle into somebody else's: you learn more,
you understand what you are using, and the tool ends up doing what you actually
need instead of what someone assumed you needed.

Three things I did not find solved anywhere:

**Adjusting difficulty gradually.** The tabs in circulation come in two
extremes, beginner versions that bore you or note-for-note transcriptions nobody
can play. The middle is missing. I want to ask for *slightly* harder and get
sensible embellishments without the piece becoming unplayable.

**Knowing what I actually know.** Not a list of ticked songs, but how fast each
one comes out today against its real tempo, and which bars I keep stumbling on.

**Capturing without friction.** The video and the editor on one screen, with an
A-B loop, half speed and a three-second rewind all a keystroke away.

## What it does

- **Capture** from YouTube: embedded player with A-B loop, reduced speed and
  quick rewind, next to an editor with keyboard entry and a clickable fretboard.
- **Repository** of your own tabs, with search, tags and version history.
- **Progress**: per-song status, current tempo against target, and marking of
  the bars that give you trouble.
- **Printing**: a sheet readable from the music stand, with standard notation
  and optional chord diagrams.
- **Difficulty transformation**: simplify or embellish, with a validator that
  guarantees what comes out can actually be played.

## Writing tabs

Your hands never leave the keyboard. Video controls sit on the function keys so
they never collide with note entry.

| Keys | |
| --- | --- |
| arrows | move between strings and beats |
| `0`–`9` | fret; two digits in a row give 10 to 24 |
| `+` `−` `.` | shorter, longer, dotted |
| `H` `V` `P` `G` `X` `A` `L` `S` | slur, vibrato, muted, ghost, dead, accent, let ring, staccato |
| `F1`–`F4` | video: play, back 3s, half speed, A-B loop |

The A-B loop is one key: press to open, press to close, press to clear.

## Status

Under construction, but **already usable for transcribing**: open a video, type,
save.

| Milestone | | |
| --- | --- | --- |
| M0 | Tauri + alphaTab + YouTube spike | done |
| M1 | Data model and serialisation | done |
| M2 | Fast capture | in progress: keyboard, video and saving work; fretboard and synced cursor pending |
| M3 | Repertoire, progress and printing | pending |
| M4 | Difficulty scoring | pending |
| M5 | Transformation engine | pending |
| M6 | AI assistance (optional, off by default) | pending |
| M7 | Guitar Pro and MusicXML import/export | pending |

## Built with

Rust and Tauri v2 for the core and the desktop window. TypeScript and
[alphaTab](https://alphatab.net) for score rendering, the synthesiser and
printing. Local SQLite for the index and practice data.

Tabs are stored as versioned JSON files in the repository itself, so there is a
backup, a history of how each arrangement evolved, and publishing one is just a
push.

## Licence
MIT
