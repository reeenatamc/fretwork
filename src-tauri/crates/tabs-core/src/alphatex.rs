//! Serialización del modelo canónico a AlphaTex.
//!
//! alphaTab **lee** AlphaTex pero no lo escribe: el exportador existió antes de la versión
//! 0.98 y se retiró. Por eso este serializador vive aquí.
//!
//! AlphaTex es **transporte hacia el renderizador**, no formato de almacenamiento. Lo que
//! no sabe expresar —identificadores estables, procedencia de los arreglos— se queda en el
//! JSON, que es la fuente de verdad.
//!
//! Estructura del documento: metadatos, luego `.`, luego los compases separados por `|`.
//! La duración se fija con `:4` y **se arrastra** hasta que se cambie, así que sólo se
//! escribe cuando varía.

use std::fmt::Write as _;

use crate::model::{
    BrushDirection, Duration, Dynamics, Harmonic, Note, NoteTechniques, Score, SlideIn, SlideOut,
    TimeSignature, Track, TripletFeel,
};
use crate::pitch::midi_to_scientific;

/// Convierte una partitura completa a AlphaTex.
#[must_use]
pub fn to_alphatex(score: &Score) -> String {
    let mut out = String::with_capacity(1024);
    write_metadata(&mut out, score);

    for (index, track) in score.tracks.iter().enumerate() {
        write_track(&mut out, score, track, index);
    }

    out
}

/// Escribe la cabecera con los metadatos de la canción.
fn write_metadata(out: &mut String, score: &Score) {
    let meta = &score.meta;
    write_meta_line(out, "title", Some(&meta.title));
    write_meta_line(out, "subtitle", meta.subtitle.as_deref());
    write_meta_line(out, "artist", meta.artist.as_deref());
    write_meta_line(out, "album", meta.album.as_deref());
    write_meta_line(out, "words", meta.words.as_deref());
    write_meta_line(out, "music", meta.music.as_deref());
    write_meta_line(out, "tab", meta.tab_author.as_deref());

    if meta.tempo_bpm > 0.0 {
        let _ = writeln!(out, "\\tempo {}", trim_float(meta.tempo_bpm));
    }
}

fn write_meta_line(out: &mut String, tag: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|v| !v.is_empty()) {
        let _ = writeln!(out, "\\{tag} \"{}\"", escape(value));
    }
}

/// Escribe una pista con su afinación y todos sus compases.
fn write_track(out: &mut String, score: &Score, track: &Track, index: usize) {
    // La primera pista puede omitir la cabecera si es la única, pero escribirla siempre
    // hace el resultado más predecible y más fácil de depurar.
    let _ = writeln!(
        out,
        "\\track \"{}\" \"{}\" {{instrument {} }}",
        escape(&track.name),
        escape(&track.short_name),
        track.midi_program
    );

    for staff in &track.staves {
        let mut flags = Vec::new();
        if staff.show_standard {
            flags.push("score");
        }
        if staff.show_tabs {
            flags.push("tabs");
        }
        let _ = writeln!(out, "\\staff {{{}}}", flags.join(" "));

        let tuning: Vec<String> = track
            .tuning
            .midi_notes
            .iter()
            .copied()
            .map(midi_to_scientific)
            .collect();
        let _ = writeln!(out, "\\tuning {}", tuning.join(" "));

        if track.capo > 0 {
            let _ = writeln!(out, "\\capo {}", track.capo);
        }

        // Cuántas voces hay que emitir: AlphaTex las escribe una detrás de otra.
        let voice_count = staff
            .bars
            .iter()
            .map(|bar| bar.voices.len())
            .max()
            .unwrap_or(1);

        for voice_index in 0..voice_count {
            if voice_index > 0 {
                let _ = writeln!(out, "\\voice");
            }
            write_voice(out, score, staff, voice_index, index);
        }
    }
}

/// Escribe todos los compases de una voz concreta.
fn write_voice(
    out: &mut String,
    score: &Score,
    staff: &crate::model::Staff,
    voice_index: usize,
    _track_index: usize,
) {
    // La duración se arrastra entre pulsos y entre compases; se emite sólo al cambiar.
    let mut current_duration: Option<Duration> = None;

    for (bar_index, bar) in staff.bars.iter().enumerate() {
        let mut bar_text = String::new();

        // Los metadatos del compás salen de MasterBar, que es común a todas las pistas.
        if let Some(master) = score.master_bars.get(bar_index) {
            write_bar_metadata(&mut bar_text, master, bar_index, score);
        }

        let beats = bar
            .voices
            .get(voice_index)
            .map_or(&[][..], |voice| voice.beats.as_slice());

        if beats.is_empty() {
            // Un compás vacío no es "nada": es un compás de silencio. Si se escribe como
            // una barra suelta, alphaTab lo descarta y la partitura pierde compases.
            let signature = score
                .master_bars
                .get(bar_index)
                .map_or(TimeSignature::default(), |master| master.time_signature);
            write_full_bar_rest(&mut bar_text, signature, &mut current_duration);
        } else {
            for beat in beats {
                write_beat(&mut bar_text, beat, &mut current_duration);
            }
        }

        let _ = write!(out, "{bar_text}|");
        // Un salto por compás mantiene el resultado legible y los diffs pequeños.
        out.push('\n');
    }
}

/// Rellena un compás vacío con silencios que suman exactamente su duración.
///
/// Se escriben tantos silencios como indique el numerador, con la figura del denominador:
/// en 4/4 salen cuatro silencios de negra, en 6/8 seis de corchea. Así el compás siempre
/// queda exacto sea cual sea la indicación.
fn write_full_bar_rest(
    out: &mut String,
    signature: TimeSignature,
    current_duration: &mut Option<Duration>,
) {
    let duration = Duration::from_denominator(signature.denominator);

    if *current_duration != Some(duration) {
        let _ = write!(out, ":{} ", duration as u16);
        *current_duration = Some(duration);
    }

    for _ in 0..signature.numerator.max(1) {
        out.push_str("r ");
    }
}

/// Escribe los cambios de compás, tonalidad, tempo y repeticiones.
fn write_bar_metadata(
    out: &mut String,
    master: &crate::model::MasterBar,
    bar_index: usize,
    score: &Score,
) {
    let previous = bar_index
        .checked_sub(1)
        .and_then(|i| score.master_bars.get(i));

    // La indicación de compás sólo se escribe cuando cambia.
    let signature_changed =
        previous.map_or(true, |prev| prev.time_signature != master.time_signature);
    if signature_changed {
        let _ = write!(
            out,
            "\\ts {} {} ",
            master.time_signature.numerator, master.time_signature.denominator
        );
    }

    let key_changed = previous.map_or(true, |prev| prev.key_signature != master.key_signature);
    if key_changed {
        let _ = write!(out, "\\ks {} ", key_name(master.key_signature));
    }

    if let Some(tempo) = &master.tempo {
        let _ = write!(out, "\\tempo {} ", trim_float(tempo.bpm));
    }

    let feel_changed = previous.map_or(master.triplet_feel != TripletFeel::None, |prev| {
        prev.triplet_feel != master.triplet_feel
    });
    if feel_changed {
        let _ = write!(out, "\\tf {} ", triplet_feel_name(master.triplet_feel));
    }

    if let Some(section) = &master.section {
        match &section.marker {
            Some(marker) => {
                let _ = write!(
                    out,
                    "\\section \"{}\" \"{}\" ",
                    escape(marker),
                    escape(&section.text)
                );
            }
            None => {
                let _ = write!(out, "\\section \"{}\" ", escape(&section.text));
            }
        }
    }

    if master.repeat_start {
        out.push_str("\\ro ");
    }
    if master.repeat_count > 0 {
        let _ = write!(out, "\\rc {} ", master.repeat_count);
    }
    if master.free_time {
        out.push_str("\\ft ");
    }
    if master.anacrusis {
        out.push_str("\\ac ");
    }
}

/// Escribe un pulso: notas o silencio, duración y efectos.
fn write_beat(
    out: &mut String,
    beat: &crate::model::Beat,
    current_duration: &mut Option<Duration>,
) {
    // La duración es "pegajosa": sólo se emite si cambió respecto al pulso anterior.
    if *current_duration != Some(beat.duration) {
        let _ = write!(out, ":{} ", beat.duration as u16);
        *current_duration = Some(beat.duration);
    }

    if beat.is_rest {
        out.push('r');
    } else {
        match beat.notes.len() {
            0 => out.push('r'),
            1 => write_note(out, &beat.notes[0]),
            _ => {
                out.push('(');
                for (index, note) in beat.notes.iter().enumerate() {
                    if index > 0 {
                        out.push(' ');
                    }
                    write_note(out, note);
                }
                out.push(')');
            }
        }
    }

    write_beat_effects(out, beat);
    out.push(' ');
}

/// Escribe los efectos que afectan al pulso entero.
fn write_beat_effects(out: &mut String, beat: &crate::model::Beat) {
    let mut effects: Vec<String> = Vec::new();

    for _ in 0..beat.dots {
        effects.push("d".to_owned());
    }

    if let Some(tuplet) = beat.tuplet {
        // Los grupos habituales llevan forma corta; el resto se escribe completo.
        if tuplet.denominator == 2 && tuplet.numerator == 3 {
            effects.push("tu 3".to_owned());
        } else {
            effects.push(format!("tu {} {}", tuplet.numerator, tuplet.denominator));
        }
    }

    if beat.dynamics != Dynamics::MF {
        effects.push(format!("dy {}", dynamics_name(beat.dynamics)));
    }

    if let Some(chord) = &beat.chord {
        effects.push(format!("ch \"{}\"", escape(chord)));
    }

    if let Some(text) = &beat.text {
        effects.push(format!("txt \"{}\"", escape(text)));
    }

    let fx = &beat.effects;
    if fx.palm_mute {
        effects.push("pm".to_owned());
    }
    if fx.let_ring {
        effects.push("lr".to_owned());
    }
    if fx.tap {
        effects.push("tt".to_owned());
    }
    if let Some((direction, duration)) = fx.brush {
        let tag = match direction {
            BrushDirection::Up => "bu",
            BrushDirection::Down => "bd",
        };
        effects.push(format!("{tag} {duration}"));
    }
    if let Some((direction, duration)) = fx.arpeggio {
        let tag = match direction {
            BrushDirection::Up => "au",
            BrushDirection::Down => "ad",
        };
        effects.push(format!("{tag} {duration}"));
    }
    if let Some(direction) = fx.pick_stroke {
        effects.push(
            match direction {
                BrushDirection::Up => "su",
                BrushDirection::Down => "sd",
            }
            .to_owned(),
        );
    }
    if let Some((fret, full)) = fx.barre {
        effects.push(format!(
            "barre {fret} {}",
            if full { "full" } else { "half" }
        ));
    }
    if let Some(duration) = fx.tremolo_picking {
        effects.push(format!("tp {}", duration as u16));
    }

    if !effects.is_empty() {
        let _ = write!(out, "{{{}}}", effects.join(" "));
    }
}

/// Escribe una nota: `traste.cuerda` y sus efectos.
fn write_note(out: &mut String, note: &Note) {
    if note.techniques.contains(NoteTechniques::DEAD) {
        // Una nota muerta se escribe con `x` en lugar del traste.
        let _ = write!(out, "x.{}", note.string);
    } else {
        let _ = write!(out, "{}.{}", note.fret, note.string);
    }

    let mut effects: Vec<String> = Vec::new();
    let t = note.techniques;

    if t.contains(NoteTechniques::HAMMER_PULL) {
        effects.push("h".to_owned());
    }
    if t.contains(NoteTechniques::GHOST) {
        effects.push("g".to_owned());
    }
    if t.contains(NoteTechniques::PALM_MUTE) {
        effects.push("pm".to_owned());
    }
    if t.contains(NoteTechniques::LET_RING) {
        effects.push("lr".to_owned());
    }
    if t.contains(NoteTechniques::STACCATO) {
        effects.push("st".to_owned());
    }
    if t.contains(NoteTechniques::ACCENT) {
        effects.push("ac".to_owned());
    }
    if t.contains(NoteTechniques::HEAVY_ACCENT) {
        effects.push("hac".to_owned());
    }
    if t.contains(NoteTechniques::VIBRATO) {
        effects.push("v".to_owned());
    }
    if t.contains(NoteTechniques::VIBRATO_WIDE) {
        effects.push("vw".to_owned());
    }
    if note.tie_destination {
        effects.push("t".to_owned());
    }

    if let Some(points) = &note.bend {
        let values: Vec<String> = points.iter().map(|p| p.value.to_string()).collect();
        effects.push(format!("b ({})", values.join(" ")));
    }

    if let Some(slide) = note.slide_in {
        effects.push(
            match slide {
                SlideIn::FromBelow => "sib",
                SlideIn::FromAbove => "sia",
            }
            .to_owned(),
        );
    }

    if let Some(slide) = note.slide_out {
        effects.push(
            match slide {
                SlideOut::Legato => "sl",
                SlideOut::Shift => "ss",
                SlideOut::OutUp => "sou",
                SlideOut::OutDown => "sod",
            }
            .to_owned(),
        );
    }

    if let Some(harmonic) = note.harmonic {
        effects.push(match harmonic {
            Harmonic::Natural => "nh".to_owned(),
            Harmonic::Artificial(fret) => format!("ah {fret}"),
            Harmonic::Pinch => "ph".to_owned(),
            Harmonic::Tap(fret) => format!("th {fret}"),
        });
    }

    if let Some((fret, speed)) = note.trill {
        effects.push(format!("tr {fret} {}", speed as u16));
    }

    if !effects.is_empty() {
        let _ = write!(out, "{{{}}}", effects.join(" "));
    }
}

// ─────────────────────────────────────────────────────────── Auxiliares

/// Nombre de la tonalidad tal como lo espera AlphaTex.
fn key_name(key: crate::pitch::KeySignature) -> String {
    use crate::pitch::KeyMode;
    // AlphaTex acepta el nombre de la tónica; el modo menor lleva sufijo.
    let tonic = key.tonic();
    let name = match key.mode {
        KeyMode::Major => tonic.name_flat().to_owned(),
        KeyMode::Minor => format!("{}m", tonic.name_flat()),
    };
    name
}

fn triplet_feel_name(feel: TripletFeel) -> &'static str {
    match feel {
        TripletFeel::None => "none",
        TripletFeel::Triplet8th => "triplet8th",
        TripletFeel::Triplet16th => "triplet16th",
        TripletFeel::Dotted8th => "dotted8th",
        TripletFeel::Dotted16th => "dotted16th",
    }
}

fn dynamics_name(dynamics: Dynamics) -> &'static str {
    match dynamics {
        Dynamics::PPP => "ppp",
        Dynamics::PP => "pp",
        Dynamics::P => "p",
        Dynamics::MP => "mp",
        Dynamics::MF => "mf",
        Dynamics::F => "f",
        Dynamics::FF => "ff",
        Dynamics::FFF => "fff",
    }
}

/// Escapa las comillas para que no rompan una cadena de AlphaTex.
fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Formatea un número quitando los decimales que no aportan.
fn trim_float(value: f32) -> String {
    if (value.fract()).abs() < f32::EPSILON {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::to_alphatex;
    use crate::model::{
        Beat, BeatId, Duration, Note, NoteId, NoteTechniques, Score, TimeSignature,
    };

    /// Crea una partitura con un compás y los pulsos indicados.
    fn score_with_beats(beats: Vec<Beat>) -> Score {
        let mut score = Score::new("Prueba", 1);
        score.meta.tempo_bpm = 90.0;
        score.tracks[0].staves[0].bars[0].voices[0].beats = beats;
        score
    }

    #[test]
    fn se_escriben_los_metadatos() {
        let mut score = Score::new("Wish You Were Here", 1);
        score.meta.artist = Some("Pink Floyd".to_owned());
        score.meta.tempo_bpm = 63.0;

        let tex = to_alphatex(&score);
        assert!(tex.contains("\\title \"Wish You Were Here\""));
        assert!(tex.contains("\\artist \"Pink Floyd\""));
        assert!(tex.contains("\\tempo 63"));
    }

    #[test]
    fn una_nota_se_escribe_traste_punto_cuerda() {
        let mut beat = Beat::new(BeatId(1), Duration::Quarter);
        beat.notes.push(Note::new(NoteId(2), 3, 5));
        let tex = to_alphatex(&score_with_beats(vec![beat]));
        assert!(
            tex.contains("5.3"),
            "traste 5 en la cuerda 3 se escribe 5.3; salió:\n{tex}"
        );
    }

    #[test]
    fn un_acorde_va_entre_parentesis() {
        let mut beat = Beat::new(BeatId(1), Duration::Quarter);
        beat.notes.push(Note::new(NoteId(2), 3, 0));
        beat.notes.push(Note::new(NoteId(3), 2, 1));
        beat.notes.push(Note::new(NoteId(4), 1, 2));
        let tex = to_alphatex(&score_with_beats(vec![beat]));
        assert!(tex.contains("(0.3 1.2 2.1)"), "salió:\n{tex}");
    }

    #[test]
    fn la_duracion_se_arrastra_y_solo_se_escribe_al_cambiar() {
        let beats = vec![
            Beat::new(BeatId(1), Duration::Quarter),
            Beat::new(BeatId(2), Duration::Quarter),
            Beat::new(BeatId(3), Duration::Eighth),
            Beat::new(BeatId(4), Duration::Eighth),
        ];
        let tex = to_alphatex(&score_with_beats(beats));
        assert_eq!(
            tex.matches(":4").count(),
            1,
            "la negra se declara una vez; salió:\n{tex}"
        );
        assert_eq!(
            tex.matches(":8").count(),
            1,
            "la corchea se declara una vez; salió:\n{tex}"
        );
    }

    #[test]
    fn el_silencio_se_escribe_con_r() {
        let tex = to_alphatex(&score_with_beats(vec![Beat::rest(
            BeatId(1),
            Duration::Quarter,
        )]));
        assert!(tex.contains('r'));
    }

    #[test]
    fn las_tecnicas_van_entre_llaves() {
        let mut beat = Beat::new(BeatId(1), Duration::Eighth);
        let mut note = Note::new(NoteId(2), 2, 5);
        note.techniques = NoteTechniques::HAMMER_PULL | NoteTechniques::VIBRATO;
        beat.notes.push(note);
        let tex = to_alphatex(&score_with_beats(vec![beat]));
        assert!(tex.contains("5.2{h v}"), "salió:\n{tex}");
    }

    #[test]
    fn la_nota_muerta_lleva_equis_en_vez_de_traste() {
        let mut beat = Beat::new(BeatId(1), Duration::Eighth);
        let mut note = Note::new(NoteId(2), 4, 7);
        note.techniques = NoteTechniques::DEAD;
        beat.notes.push(note);
        let tex = to_alphatex(&score_with_beats(vec![beat]));
        assert!(tex.contains("x.4"), "salió:\n{tex}");
    }

    /// Cuenta los silencios que hay en las líneas de compás, ignorando la cabecera.
    ///
    /// La cabecera trae letras `r` en palabras como `Guitarra` o `instrument`, así que
    /// contarlas sobre el texto entero daría un número engañoso.
    fn count_rests(tex: &str) -> usize {
        tex.lines()
            .filter(|line| line.trim_end().ends_with('|'))
            .flat_map(|line| line.split_whitespace())
            .filter(|token| *token == "r" || *token == "r|")
            .count()
    }

    #[test]
    fn un_compas_vacio_se_rellena_con_silencios() {
        // Regresión: escribir un compás vacío como una barra suelta hacía que alphaTab
        // lo descartara, y una partitura de 4 compases se renderizaba como 1.
        let tex = to_alphatex(&Score::new("Prueba", 4));
        assert_eq!(
            tex.matches('|').count(),
            4,
            "cuatro compases; salió:\n{tex}"
        );
        assert_eq!(
            count_rests(&tex),
            16,
            "cuatro silencios por compás; salió:\n{tex}"
        );
    }

    #[test]
    fn el_relleno_respeta_la_indicacion_de_compas() {
        let mut score = Score::new("Prueba", 1);
        score.master_bars[0].time_signature = TimeSignature {
            numerator: 6,
            denominator: 8,
        };
        let tex = to_alphatex(&score);
        assert!(
            tex.contains(":8"),
            "en 6/8 los silencios son de corchea; salió:\n{tex}"
        );
        assert_eq!(count_rests(&tex), 6, "seis silencios; salió:\n{tex}");
    }

    #[test]
    fn cada_compas_termina_en_barra() {
        let score = Score::new("Prueba", 3);
        let tex = to_alphatex(&score);
        assert_eq!(
            tex.matches('|').count(),
            3,
            "tres compases, tres barras; salió:\n{tex}"
        );
    }

    #[test]
    fn la_indicacion_de_compas_solo_se_escribe_al_cambiar() {
        let mut score = Score::new("Prueba", 3);
        score.master_bars[2].time_signature = TimeSignature {
            numerator: 3,
            denominator: 4,
        };
        let tex = to_alphatex(&score);
        assert!(
            tex.contains("\\ts 4 4"),
            "el compás inicial se declara; salió:\n{tex}"
        );
        assert!(
            tex.contains("\\ts 3 4"),
            "el cambio se declara; salió:\n{tex}"
        );
        assert_eq!(
            tex.matches("\\ts").count(),
            2,
            "sólo dos veces; salió:\n{tex}"
        );
    }

    #[test]
    fn la_afinacion_se_escribe_de_aguda_a_grave() {
        let tex = to_alphatex(&Score::new("Prueba", 1));
        assert!(
            tex.contains("\\tuning E4 B3 G3 D3 A2 E2"),
            "la 1ª cuerda va primero; salió:\n{tex}"
        );
    }

    #[test]
    fn la_cejilla_se_escribe_solo_si_existe() {
        let sin_cejilla = to_alphatex(&Score::new("Prueba", 1));
        assert!(!sin_cejilla.contains("\\capo"));

        let mut score = Score::new("Prueba", 1);
        score.tracks[0].capo = 3;
        assert!(to_alphatex(&score).contains("\\capo 3"));
    }

    #[test]
    fn las_comillas_del_titulo_se_escapan() {
        let mut score = Score::new(r#"Cancion "rara""#, 1);
        score.meta.tempo_bpm = 100.0;
        let tex = to_alphatex(&score);
        assert!(tex.contains(r#"\"rara\""#), "salió:\n{tex}");
    }
}
