//! Modelo canónico de una partitura.
//!
//! Jerarquía: [`Score`] → [`Track`] → [`Staff`] → [`Bar`] → [`Voice`] → [`Beat`] → [`Note`],
//! con los datos comunes a todas las pistas en [`MasterBar`].
//!
//! Cuatro decisiones estructurales que hay que respetar:
//!
//! 1. **Separación entre [`MasterBar`] y [`Bar`].** El compás, la tonalidad, el tempo y las
//!    repeticiones son globales por índice de compás; las notas viven en el [`Bar`] de cada
//!    pista. Sin esta separación, las partituras de varias pistas se desincronizan al editar.
//! 2. **La cuerda 1 es la más aguda** (ver [`crate::pitch`]).
//! 3. **Cada [`Beat`] y [`Note`] lleva un identificador estable.** El motor de transformación,
//!    el deshacer y el validador direccionan elementos concretos; hacerlo por posición se rompe
//!    en cuanto una transformación inserta o quita un pulso.
//! 4. **El traste es relativo a la cejilla.**

use bitflags::bitflags;
use serde::{Deserialize, Serialize};

use crate::pitch::{KeySignature, Tuning};

/// Versión del esquema serializado. Se incrementa ante cambios incompatibles.
pub const SCHEMA_VERSION: u32 = 1;

/// Ayuda a omitir las banderas apagadas al serializar.
///
/// Los archivos de canción se leen en un `git diff`, así que escribir decenas de
/// `"repeat_start": false` por compás sólo sirve para enterrar el cambio de verdad.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(value: &bool) -> bool {
    !*value
}

/// Igual que [`is_false`], para los contadores que casi siempre valen cero.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero_u8(value: &u8) -> bool {
    *value == 0
}

// ─────────────────────────────────────────────────────────── Identificadores

/// Identificador estable de un pulso dentro de una partitura.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BeatId(pub u64);

/// Identificador estable de una nota dentro de una partitura.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NoteId(pub u64);

/// Dirección de un pulso: dónde vive dentro de la jerarquía.
///
/// Es lo que usan la interfaz de usuario y la capa de IA para señalar un sitio concreto.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BeatAddr {
    /// Índice de pista.
    pub track: u32,
    /// Índice de pentagrama dentro de la pista.
    pub staff: u32,
    /// Índice de compás.
    pub bar: u32,
    /// Índice de voz dentro del compás.
    pub voice: u32,
    /// Índice del pulso dentro de la voz.
    pub beat: u32,
}

// ─────────────────────────────────────────────────────────── Partitura

/// Una partitura completa.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Score {
    /// Versión del esquema con el que se serializó.
    pub schema_version: u32,
    /// Identificador único y estable de la canción.
    pub id: uuid::Uuid,
    /// Metadatos: título, artista, origen.
    pub meta: ScoreMeta,
    /// Datos por compás compartidos por todas las pistas.
    pub master_bars: Vec<MasterBar>,
    /// Pistas instrumentales.
    pub tracks: Vec<Track>,
    /// Secciones lógicas (Intro, Verso, Coro…).
    pub sections: Vec<Section>,
    /// Puntos de sincronía con un vídeo o audio externo.
    #[serde(default)]
    pub sync_points: Vec<SyncPoint>,
    /// Siguiente identificador libre. Garantiza que los ids no se repitan.
    #[serde(default)]
    pub next_id: u64,
}

impl Score {
    /// Crea una partitura vacía con una pista de guitarra y el número de compases indicado.
    #[must_use]
    pub fn new(title: impl Into<String>, bar_count: u32) -> Self {
        let mut score = Self {
            schema_version: SCHEMA_VERSION,
            id: uuid::Uuid::new_v4(),
            meta: ScoreMeta {
                title: title.into(),
                ..ScoreMeta::default()
            },
            master_bars: Vec::new(),
            tracks: vec![Track::default()],
            sections: Vec::new(),
            sync_points: Vec::new(),
            next_id: 1,
        };
        for index in 0..bar_count {
            score.master_bars.push(MasterBar {
                index,
                ..MasterBar::default()
            });
        }
        for track in &mut score.tracks {
            for staff in &mut track.staves {
                staff.bars = (0..bar_count).map(|_| Bar::default()).collect();
            }
        }
        score.assign_missing_ids();
        score
    }

    /// Reserva un identificador de pulso nuevo.
    pub fn next_beat_id(&mut self) -> BeatId {
        let id = BeatId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Reserva un identificador de nota nuevo.
    pub fn next_note_id(&mut self) -> NoteId {
        let id = NoteId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Asigna identificadores a los pulsos y notas que aún no lo tengan.
    ///
    /// Se llama tras deserializar o importar, donde los ids pueden venir a cero.
    pub fn assign_missing_ids(&mut self) {
        let mut next = self.next_id.max(1);
        for track in &mut self.tracks {
            for staff in &mut track.staves {
                for bar in &mut staff.bars {
                    for voice in &mut bar.voices {
                        for beat in &mut voice.beats {
                            if beat.id.0 == 0 {
                                beat.id = BeatId(next);
                                next += 1;
                            }
                            for note in &mut beat.notes {
                                if note.id.0 == 0 {
                                    note.id = NoteId(next);
                                    next += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        self.next_id = next;
    }

    /// Número de compases de la partitura.
    #[must_use]
    pub fn bar_count(&self) -> u32 {
        // Una partitura no llega a 4 mil millones de compases.
        self.master_bars.len() as u32
    }

    /// Devuelve el pulso en una dirección concreta, si existe.
    #[must_use]
    pub fn beat_at(&self, addr: BeatAddr) -> Option<&Beat> {
        self.tracks
            .get(addr.track as usize)?
            .staves
            .get(addr.staff as usize)?
            .bars
            .get(addr.bar as usize)?
            .voices
            .get(addr.voice as usize)?
            .beats
            .get(addr.beat as usize)
    }

    /// Devuelve el pulso en una dirección concreta para modificarlo, si existe.
    pub fn beat_at_mut(&mut self, addr: BeatAddr) -> Option<&mut Beat> {
        self.tracks
            .get_mut(addr.track as usize)?
            .staves
            .get_mut(addr.staff as usize)?
            .bars
            .get_mut(addr.bar as usize)?
            .voices
            .get_mut(addr.voice as usize)?
            .beats
            .get_mut(addr.beat as usize)
    }

    /// Recorre todos los pulsos junto con su dirección.
    pub fn iter_beats(&self) -> impl Iterator<Item = (BeatAddr, &Beat)> {
        self.tracks.iter().enumerate().flat_map(move |(t, track)| {
            track.staves.iter().enumerate().flat_map(move |(s, staff)| {
                staff.bars.iter().enumerate().flat_map(move |(b, bar)| {
                    bar.voices.iter().enumerate().flat_map(move |(v, voice)| {
                        voice.beats.iter().enumerate().map(move |(i, beat)| {
                            let addr = BeatAddr {
                                track: t as u32,
                                staff: s as u32,
                                bar: b as u32,
                                voice: v as u32,
                                beat: i as u32,
                            };
                            (addr, beat)
                        })
                    })
                })
            })
        })
    }
}

/// Metadatos de una partitura.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct ScoreMeta {
    /// Título de la canción.
    pub title: String,
    /// Subtítulo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Intérprete o grupo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    /// Álbum.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    /// Autor de la letra.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words: Option<String>,
    /// Autor de la música.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub music: Option<String>,
    /// Quien transcribió la tablatura.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_author: Option<String>,
    /// Enlace del vídeo de YouTube del que se transcribió. Sirve de atribución.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    /// Pulsaciones por minuto de la grabación original.
    pub tempo_bpm: f32,
    /// Etiquetas para encontrarla en el repertorio: estilo, técnica, para quién es.
    ///
    /// Viven con la canción y no en un índice aparte: al publicar la tablatura, la
    /// etiqueta va con ella y sigue significando lo mismo en otro ordenador.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// Limpia una lista de etiquetas: sin espacios de sobra, en minúsculas y sin repetidas.
///
/// Las etiquetas se escriben a mano, y a mano se escriben mal: «Fingerstyle», «fingerstyle»
/// y « fingerstyle » son la misma etiqueta y en el repertorio tienen que caer juntas.
#[must_use]
pub fn normalize_tags(tags: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut clean: Vec<String> = Vec::new();
    for tag in tags {
        let tag = tag.trim().to_lowercase();
        if tag.is_empty() || clean.contains(&tag) {
            continue;
        }
        clean.push(tag);
    }
    clean
}

// ─────────────────────────────────────────────────────────── Compás maestro

/// Datos de un compás compartidos por todas las pistas.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct MasterBar {
    /// Posición del compás dentro de la partitura, empezando en cero.
    pub index: u32,
    /// Indicación de compás.
    pub time_signature: TimeSignature,
    /// Armadura.
    pub key_signature: KeySignature,
    /// Cambio de tempo que empieza en este compás.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tempo: Option<TempoChange>,
    /// Interpretación de swing.
    #[serde(default, skip_serializing_if = "TripletFeel::is_none")]
    pub triplet_feel: TripletFeel,
    /// Aquí abre una repetición.
    #[serde(default, skip_serializing_if = "is_false")]
    pub repeat_start: bool,
    /// Número de repeticiones. Cero significa que no cierra ninguna.
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub repeat_count: u8,
    /// Máscara de casillas de final alternativo.
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub alternate_endings: u8,
    /// Marcador de sección que empieza aquí.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<SectionMarker>,
    /// Compás sin pulso definido.
    #[serde(default, skip_serializing_if = "is_false")]
    pub free_time: bool,
    /// Compás de anacrusa.
    #[serde(default, skip_serializing_if = "is_false")]
    pub anacrusis: bool,
    /// Doble barra al final.
    #[serde(default, skip_serializing_if = "is_false")]
    pub double_bar: bool,
}

/// Indicación de compás, por ejemplo 4/4.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeSignature {
    /// Pulsos por compás.
    pub numerator: u8,
    /// Figura que vale un pulso.
    pub denominator: u8,
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self {
            numerator: 4,
            denominator: 4,
        }
    }
}

/// Cambio de tempo.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TempoChange {
    /// Pulsaciones por minuto.
    pub bpm: f32,
    /// Etiqueta opcional, por ejemplo `"Lento"`.
    pub label: Option<String>,
}

/// Interpretación rítmica de swing.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TripletFeel {
    /// Sin swing: las figuras suenan tal como están escritas.
    #[default]
    None,
    /// Corcheas con swing de tresillo.
    Triplet8th,
    /// Semicorcheas con swing de tresillo.
    Triplet16th,
    /// Corcheas con puntillo.
    Dotted8th,
    /// Semicorcheas con puntillo.
    Dotted16th,
}

impl TripletFeel {
    /// ¿Es la interpretación recta, sin swing?
    #[must_use]
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

/// Marcador de sección escrito sobre el compás.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SectionMarker {
    /// Texto visible, por ejemplo `"Coro"`.
    pub text: String,
    /// Abreviatura opcional, por ejemplo `"C"`.
    pub marker: Option<String>,
}

/// Sección lógica de la canción, para navegar y para repartir el presupuesto de arreglos.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Section {
    /// Identificador dentro de la partitura.
    pub id: u32,
    /// Tipo de sección.
    pub kind: SectionKind,
    /// Nombre visible, por ejemplo `"Coro 1"`.
    pub label: String,
    /// Primer compás de la sección, incluido.
    pub bar_start: u32,
    /// Compás siguiente al último, excluido.
    pub bar_end: u32,
}

/// Tipos de sección habituales en una canción.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionKind {
    /// Introducción.
    Intro,
    /// Estrofa.
    Verso,
    /// Pre-estribillo.
    PreCoro,
    /// Estribillo.
    Coro,
    /// Puente.
    Puente,
    /// Solo instrumental.
    Solo,
    /// Interludio.
    Interludio,
    /// Cierre.
    Outro,
    /// Cualquier otra cosa.
    Otro,
}

/// Punto de sincronía entre la partitura y un vídeo o audio externo.
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct SyncPoint {
    /// Compás al que corresponde.
    pub bar_index: u32,
    /// Qué pasada por ese compás, contando repeticiones.
    pub occurence: u32,
    /// Posición dentro del compás, de 0.0 a 1.0.
    pub ratio_position: f32,
    /// Instante del medio externo, en milisegundos.
    pub millisecond_offset: f64,
}

// ─────────────────────────────────────────────────────────── Pista

/// Una pista instrumental.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Track {
    /// Nombre completo.
    pub name: String,
    /// Nombre abreviado, para los márgenes de la partitura.
    pub short_name: String,
    /// Programa General MIDI. 25 es guitarra de nailon, 26 de acero, 29 con distorsión.
    pub midi_program: u8,
    /// Canal MIDI.
    pub midi_channel: u8,
    /// Afinación del instrumento.
    pub tuning: Tuning,
    /// Traste donde está puesta la cejilla. Cero si no hay.
    pub capo: u8,
    /// Número de trastes del instrumento. Límite duro para el validador.
    pub fret_count: u8,
    /// Pentagramas de la pista.
    pub staves: Vec<Staff>,
}

impl Default for Track {
    fn default() -> Self {
        Self {
            name: "Guitarra".to_owned(),
            short_name: "Gtr".to_owned(),
            midi_program: 25,
            midi_channel: 0,
            tuning: Tuning::standard(),
            capo: 0,
            fret_count: 22,
            staves: vec![Staff::default()],
        }
    }
}

/// Un pentagrama dentro de una pista.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Staff {
    /// Mostrar notación estándar.
    pub show_standard: bool,
    /// Mostrar tablatura.
    pub show_tabs: bool,
    /// Compases. Su longitud coincide con la de [`Score::master_bars`].
    pub bars: Vec<Bar>,
}

impl Default for Staff {
    fn default() -> Self {
        Self {
            show_standard: false,
            show_tabs: true,
            bars: Vec::new(),
        }
    }
}

/// Un compás de un pentagrama.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Bar {
    /// Voces simultáneas. La voz cero existe siempre.
    pub voices: Vec<Voice>,
}

impl Default for Bar {
    fn default() -> Self {
        Self {
            voices: vec![Voice::default()],
        }
    }
}

/// Una voz dentro de un compás.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Voice {
    /// Pulsos en orden temporal.
    pub beats: Vec<Beat>,
}

// ─────────────────────────────────────────────────────────── Pulso y nota

/// Un pulso: un ataque simultáneo de cero o más notas, con su duración.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Beat {
    /// Identificador estable.
    pub id: BeatId,
    /// Figura rítmica.
    pub duration: Duration,
    /// Número de puntillos.
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub dots: u8,
    /// Grupo irregular al que pertenece, por ejemplo un tresillo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tuplet: Option<Tuplet>,
    /// Es un silencio.
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_rest: bool,
    /// Notas que suenan. Vacío si es silencio.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<Note>,
    /// Matiz dinámico.
    #[serde(default, skip_serializing_if = "Dynamics::is_default")]
    pub dynamics: Dynamics,
    /// Efectos que afectan al pulso entero.
    #[serde(default, skip_serializing_if = "BeatEffects::is_empty")]
    pub effects: BeatEffects,
    /// Cifrado del acorde mostrado encima, por ejemplo `"Am7"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chord: Option<String>,
    /// Texto libre sobre el pulso.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// De qué transformación salió este pulso, si salió de alguna.
    #[serde(default)]
    pub provenance: Option<Provenance>,
}

impl Beat {
    /// Crea un silencio con la figura indicada.
    #[must_use]
    pub fn rest(id: BeatId, duration: Duration) -> Self {
        Self {
            is_rest: true,
            ..Self::new(id, duration)
        }
    }

    /// Crea un pulso vacío con la figura indicada.
    #[must_use]
    pub fn new(id: BeatId, duration: Duration) -> Self {
        Self {
            id,
            duration,
            dots: 0,
            tuplet: None,
            is_rest: false,
            notes: Vec::new(),
            dynamics: Dynamics::MF,
            effects: BeatEffects::default(),
            chord: None,
            text: None,
            provenance: None,
        }
    }

    /// Duración del pulso en redondas, teniendo en cuenta puntillos y grupos irregulares.
    ///
    /// Se calcula con enteros para evitar los errores de redondeo que harían fallar la
    /// comprobación de que un compás está bien relleno.
    #[must_use]
    pub fn duration_in_whole_notes(&self) -> Fraction {
        let mut value = Fraction::new(1, u64::from(self.duration as u16));
        // Cada puntillo añade la mitad de lo que lleve acumulado.
        let mut extra = value;
        for _ in 0..self.dots {
            extra = extra.half();
            value = value + extra;
        }
        if let Some(tuplet) = self.tuplet {
            value = value.scale(u64::from(tuplet.denominator), u64::from(tuplet.numerator));
        }
        value
    }
}

/// Figura rítmica, expresada como divisor de la redonda.
///
/// Se serializa como **número**, no como nombre de variante: es lo que espera la interfaz
/// (`4` es negra, `8` corchea) y evita una traducción extra en cada operación de edición.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(into = "u16", try_from = "u16")]
#[repr(u16)]
pub enum Duration {
    /// Redonda.
    Whole = 1,
    /// Blanca.
    Half = 2,
    /// Negra.
    Quarter = 4,
    /// Corchea.
    Eighth = 8,
    /// Semicorchea.
    Sixteenth = 16,
    /// Fusa.
    ThirtySecond = 32,
    /// Semifusa.
    SixtyFourth = 64,
}

impl From<Duration> for u16 {
    fn from(duration: Duration) -> Self {
        duration as Self
    }
}

impl TryFrom<u16> for Duration {
    type Error = String;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Whole),
            2 => Ok(Self::Half),
            4 => Ok(Self::Quarter),
            8 => Ok(Self::Eighth),
            16 => Ok(Self::Sixteenth),
            32 => Ok(Self::ThirtySecond),
            64 => Ok(Self::SixtyFourth),
            other => Err(format!("{other} no es una figura rítmica válida")),
        }
    }
}

impl Duration {
    /// Figura que corresponde al denominador de una indicación de compás.
    ///
    /// En 4/4 el denominador 4 es la negra; en 6/8, el 8 es la corchea. Los valores que no
    /// son potencia de dos válida caen en la negra, que es el supuesto más seguro.
    #[must_use]
    pub const fn from_denominator(denominator: u8) -> Self {
        match denominator {
            1 => Self::Whole,
            2 => Self::Half,
            8 => Self::Eighth,
            16 => Self::Sixteenth,
            32 => Self::ThirtySecond,
            64 => Self::SixtyFourth,
            _ => Self::Quarter,
        }
    }
}

/// Grupo irregular, por ejemplo un tresillo (3 en el espacio de 2).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tuplet {
    /// Cuántas figuras se tocan.
    pub numerator: u8,
    /// En el espacio de cuántas.
    pub denominator: u8,
}

/// Matiz dinámico.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[allow(missing_docs, clippy::doc_markdown)]
pub enum Dynamics {
    PPP,
    PP,
    P,
    MP,
    /// Matiz por defecto: la mayoría de las notas no llevan indicación.
    #[default]
    MF,
    F,
    FF,
    FFF,
}

/// Efectos aplicados al pulso completo.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct BeatEffects {
    /// Apagado con la palma.
    pub palm_mute: bool,
    /// Dejar sonar las cuerdas.
    pub let_ring: bool,
    /// Rasgueo hacia arriba o hacia abajo, con su duración en pulsos MIDI.
    pub brush: Option<(BrushDirection, u16)>,
    /// Arpegiado, con su duración en pulsos MIDI.
    pub arpeggio: Option<(BrushDirection, u16)>,
    /// Golpe de púa hacia arriba o hacia abajo.
    pub pick_stroke: Option<BrushDirection>,
    /// Cejilla con el índice: traste y si es completa.
    pub barre: Option<(u8, bool)>,
    /// Trémolo de púa, con la figura de repetición.
    pub tremolo_picking: Option<Duration>,
    /// Nota tocada con tapping.
    pub tap: bool,
}

impl BeatEffects {
    /// ¿No hay ningún efecto activo? Sirve para no escribirlos cuando están vacíos.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

impl Dynamics {
    /// ¿Es el matiz por defecto?
    #[must_use]
    pub fn is_default(&self) -> bool {
        matches!(self, Self::MF)
    }
}

/// Dirección de un rasgueo o arpegiado.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrushDirection {
    /// De la cuerda grave a la aguda.
    Up,
    /// De la cuerda aguda a la grave.
    Down,
}

/// Una nota dentro de un pulso.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Note {
    /// Identificador estable.
    pub id: NoteId,
    /// Cuerda, donde 1 es la más aguda.
    pub string: u8,
    /// Traste, relativo a la cejilla. Cero es al aire.
    pub fret: u8,
    /// Técnicas de mano izquierda y derecha.
    #[serde(default, skip_serializing_if = "NoteTechniques::is_empty")]
    pub techniques: NoteTechniques,
    /// Curva de bend, como lista de puntos.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bend: Option<Vec<BendPoint>>,
    /// Entrada por glissando.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slide_in: Option<SlideIn>,
    /// Salida por glissando.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slide_out: Option<SlideOut>,
    /// Armónico.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harmonic: Option<Harmonic>,
    /// Trino: traste destino y velocidad.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trill: Option<(u8, Duration)>,
    /// La nota anterior se prolonga en esta.
    #[serde(default, skip_serializing_if = "is_false")]
    pub tie_destination: bool,
}

impl Note {
    /// Crea una nota simple en una cuerda y traste.
    #[must_use]
    pub fn new(id: NoteId, string: u8, fret: u8) -> Self {
        Self {
            id,
            string,
            fret,
            techniques: NoteTechniques::empty(),
            bend: None,
            slide_in: None,
            slide_out: None,
            harmonic: None,
            trill: None,
            tie_destination: false,
        }
    }
}

bitflags! {
    /// Técnicas que puede llevar una nota.
    #[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct NoteTechniques: u32 {
        /// Ligado desde la nota anterior.
        const HAMMER_PULL  = 1 << 0;
        /// Nota fantasma, apagada parcialmente.
        const GHOST        = 1 << 1;
        /// Nota muerta, sin altura definida.
        const DEAD         = 1 << 2;
        /// Apagada con la palma.
        const PALM_MUTE    = 1 << 3;
        /// Dejar sonar.
        const LET_RING     = 1 << 4;
        /// Staccato.
        const STACCATO     = 1 << 5;
        /// Acentuada.
        const ACCENT       = 1 << 6;
        /// Muy acentuada.
        const HEAVY_ACCENT = 1 << 7;
        /// Vibrato.
        const VIBRATO      = 1 << 8;
        /// Vibrato amplio.
        const VIBRATO_WIDE = 1 << 9;
    }
}

/// Punto de una curva de bend.
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct BendPoint {
    /// Posición dentro de la nota, de 0 a 60.
    pub offset: u8,
    /// Altura en cuartos de tono. 4 es un tono entero.
    pub value: i8,
}

/// Glissando de entrada a una nota.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlideIn {
    /// Desde un traste inferior.
    FromBelow,
    /// Desde un traste superior.
    FromAbove,
}

/// Glissando de salida de una nota.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlideOut {
    /// Ligado hasta la nota siguiente.
    Legato,
    /// Con ataque en la nota destino.
    Shift,
    /// Hacia arriba, sin destino.
    OutUp,
    /// Hacia abajo, sin destino.
    OutDown,
}

/// Tipo de armónico.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Harmonic {
    /// Natural, rozando la cuerda sobre el traste.
    Natural,
    /// Artificial, con el traste indicado.
    Artificial(u8),
    /// De pellizco.
    Pinch,
    /// Con tapping, con el traste indicado.
    Tap(u8),
}

/// Rastro de qué transformación creó o tocó un elemento.
///
/// Es lo que permite pintar el antes y el después, y aceptar o rechazar arreglo por arreglo.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Provenance {
    /// Identificador de la transformación aplicada.
    pub move_id: String,
    /// Número de tanda, para agrupar los cambios de una misma operación.
    pub batch: u32,
    /// Si la persona lo aceptó.
    pub accepted: bool,
}

// ─────────────────────────────────────────────────────────── Fracción exacta

/// Fracción con aritmética exacta.
///
/// Las duraciones se suman con enteros a propósito: comprobar que un compás está bien
/// relleno es la verificación más barata y la que más fallos atrapa, y con coma flotante
/// los puntillos y los tresillos la volverían poco fiable.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fraction {
    /// Numerador.
    pub numerator: u64,
    /// Denominador. Nunca es cero.
    pub denominator: u64,
}

impl Fraction {
    /// Crea una fracción ya simplificada.
    ///
    /// # Panics
    ///
    /// Entra en pánico si el denominador es cero.
    #[must_use]
    pub fn new(numerator: u64, denominator: u64) -> Self {
        assert!(
            denominator != 0,
            "una fracción no puede tener denominador cero"
        );
        Self {
            numerator,
            denominator,
        }
        .reduced()
    }

    /// Fracción de valor cero.
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            numerator: 0,
            denominator: 1,
        }
    }

    /// ¿Vale cero?
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.numerator == 0
    }

    /// La mitad del valor.
    #[must_use]
    pub fn half(self) -> Self {
        Self::new(self.numerator, self.denominator * 2)
    }

    /// Multiplica por `numerator / denominator`.
    #[must_use]
    pub fn scale(self, numerator: u64, denominator: u64) -> Self {
        Self::new(self.numerator * numerator, self.denominator * denominator)
    }

    /// Valor aproximado en coma flotante. Sólo para mostrar, nunca para comparar.
    #[must_use]
    pub fn as_f64(self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }

    fn reduced(self) -> Self {
        let divisor = gcd(self.numerator, self.denominator).max(1);
        Self {
            numerator: self.numerator / divisor,
            denominator: self.denominator / divisor,
        }
    }
}

impl std::ops::Add for Fraction {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self::new(
            self.numerator * other.denominator + other.numerator * self.denominator,
            self.denominator * other.denominator,
        )
    }
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tag_tests {
    use super::normalize_tags;

    #[test]
    fn las_etiquetas_se_limpian_antes_de_guardarse() {
        let tags =
            normalize_tags(["  Fingerstyle ", "fingerstyle", "", "   ", "Blues"].map(String::from));
        assert_eq!(
            tags,
            vec!["fingerstyle", "blues"],
            "sin repetidas ni vacías"
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{Beat, BeatId, Duration, Fraction, Note, Score, TimeSignature, Tuplet};

    #[test]
    fn una_partitura_nueva_tiene_los_compases_pedidos() {
        let score = Score::new("Prueba", 8);
        assert_eq!(score.bar_count(), 8);
        assert_eq!(score.tracks.len(), 1);
        assert_eq!(score.tracks[0].staves[0].bars.len(), 8);
    }

    #[test]
    fn los_identificadores_no_se_repiten() {
        let mut score = Score::new("Prueba", 1);
        let a = score.next_beat_id();
        let b = score.next_beat_id();
        let c = score.next_note_id();
        assert_ne!(a, b);
        assert_ne!(b.0, c.0);
    }

    #[test]
    fn la_negra_dura_un_cuarto_de_redonda() {
        let beat = Beat::new(BeatId(1), Duration::Quarter);
        assert_eq!(beat.duration_in_whole_notes(), Fraction::new(1, 4));
    }

    #[test]
    fn el_puntillo_agrega_la_mitad() {
        let mut beat = Beat::new(BeatId(1), Duration::Quarter);
        beat.dots = 1;
        // Negra con puntillo = 1/4 + 1/8 = 3/8.
        assert_eq!(beat.duration_in_whole_notes(), Fraction::new(3, 8));
    }

    #[test]
    fn el_doble_puntillo_agrega_la_mitad_dos_veces() {
        let mut beat = Beat::new(BeatId(1), Duration::Quarter);
        beat.dots = 2;
        // 1/4 + 1/8 + 1/16 = 7/16.
        assert_eq!(beat.duration_in_whole_notes(), Fraction::new(7, 16));
    }

    #[test]
    fn tres_corcheas_de_tresillo_valen_una_negra() {
        let mut total = Fraction::zero();
        for _ in 0..3 {
            let mut beat = Beat::new(BeatId(1), Duration::Eighth);
            beat.tuplet = Some(Tuplet {
                numerator: 3,
                denominator: 2,
            });
            total = total + beat.duration_in_whole_notes();
        }
        assert_eq!(
            total,
            Fraction::new(1, 4),
            "un tresillo de corcheas llena una negra exacta"
        );
    }

    #[test]
    fn cuatro_negras_llenan_un_compas_de_cuatro_por_cuatro() {
        let signature = TimeSignature::default();
        let expected = Fraction::new(
            u64::from(signature.numerator),
            u64::from(signature.denominator),
        );

        let mut total = Fraction::zero();
        for _ in 0..4 {
            total = total + Beat::new(BeatId(1), Duration::Quarter).duration_in_whole_notes();
        }
        assert_eq!(total, expected);
    }

    #[test]
    fn la_fraccion_se_simplifica() {
        assert_eq!(Fraction::new(2, 8), Fraction::new(1, 4));
        assert_eq!(Fraction::new(6, 4).numerator, 3);
    }

    #[test]
    fn se_recorren_todos_los_pulsos_con_su_direccion() {
        let mut score = Score::new("Prueba", 2);
        let id = score.next_beat_id();
        let note_id = score.next_note_id();
        let mut beat = Beat::new(id, Duration::Quarter);
        beat.notes.push(Note::new(note_id, 1, 3));
        score.tracks[0].staves[0].bars[0].voices[0].beats.push(beat);

        let found: Vec<_> = score.iter_beats().collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0.bar, 0);
        assert_eq!(found[0].1.notes[0].fret, 3);
    }
}
