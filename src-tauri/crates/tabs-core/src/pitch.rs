//! Alturas, afinaciones y conversiones entre traste y nota.
//!
//! Convenciones que se respetan en todo el proyecto:
//!
//! - Las notas MIDI van de 0 a 127, con `60 = do central (C4)`.
//! - La **cuerda 1 es la más aguda** (mi agudo en afinación estándar), igual que en
//!   alphaTab y Guitar Pro. Invertir esto es el error clásico de las aplicaciones de
//!   tablatura, así que los tipos lo hacen explícito.
//! - El **traste es relativo a la cejilla**: la altura real de una nota es
//!   `afinación[cuerda - 1] + cejilla + traste`.

use serde::{Deserialize, Serialize};

/// Número de semitonos en una octava.
pub const SEMITONES_PER_OCTAVE: u8 = 12;

/// Nombres de las clases de altura, con sostenidos.
const SHARP_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Nombres de las clases de altura, con bemoles.
const FLAT_NAMES: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
];

/// Clase de altura: la nota sin octava, de 0 (do) a 11 (si).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PitchClass(u8);

impl PitchClass {
    /// Crea una clase de altura, plegando al rango 0..12.
    #[must_use]
    pub const fn new(value: u8) -> Self {
        Self(value % SEMITONES_PER_OCTAVE)
    }

    /// Clase de altura de una nota MIDI.
    #[must_use]
    pub const fn from_midi(midi: u8) -> Self {
        Self(midi % SEMITONES_PER_OCTAVE)
    }

    /// Valor numérico, de 0 a 11.
    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }

    /// Nombre con sostenidos, por ejemplo `"C#"`.
    #[must_use]
    pub fn name_sharp(self) -> &'static str {
        SHARP_NAMES[self.0 as usize]
    }

    /// Nombre con bemoles, por ejemplo `"Db"`.
    #[must_use]
    pub fn name_flat(self) -> &'static str {
        FLAT_NAMES[self.0 as usize]
    }

    /// Transporta la clase de altura por un número de semitonos con signo.
    #[must_use]
    pub const fn transpose(self, semitones: i8) -> Self {
        let raw = (self.0 as i16 + semitones as i16).rem_euclid(SEMITONES_PER_OCTAVE as i16);
        Self(raw as u8)
    }
}

/// Conjunto de clases de altura, representado como máscara de 12 bits.
///
/// Sirve para escalas y para las notas de un acorde, que es lo que consultan las
/// transformaciones al decidir si una nota añadida encaja.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PitchClassSet(u16);

impl PitchClassSet {
    /// Conjunto vacío.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Construye el conjunto a partir de clases de altura sueltas.
    #[must_use]
    pub fn from_classes(classes: &[PitchClass]) -> Self {
        let mut set = Self::empty();
        for &class in classes {
            set = set.with(class);
        }
        set
    }

    /// Devuelve el conjunto con una clase de altura añadida.
    #[must_use]
    pub const fn with(self, class: PitchClass) -> Self {
        Self(self.0 | (1 << class.0))
    }

    /// ¿Contiene esta clase de altura?
    #[must_use]
    pub const fn contains(self, class: PitchClass) -> bool {
        self.0 & (1 << class.0) != 0
    }

    /// ¿Contiene la clase de altura de esta nota MIDI?
    #[must_use]
    pub const fn contains_midi(self, midi: u8) -> bool {
        self.contains(PitchClass::from_midi(midi))
    }

    /// Número de clases distintas en el conjunto.
    #[must_use]
    pub const fn len(self) -> u32 {
        self.0.count_ones()
    }

    /// ¿Está vacío?
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Unión de dos conjuntos.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Transporta el conjunto entero por un número de semitonos con signo.
    #[must_use]
    pub fn transposed(self, semitones: i8) -> Self {
        let mut result = Self::empty();
        for value in 0..SEMITONES_PER_OCTAVE {
            let class = PitchClass::new(value);
            if self.contains(class) {
                result = result.with(class.transpose(semitones));
            }
        }
        result
    }

    /// Itera las clases de altura contenidas, de grave a aguda.
    pub fn iter(self) -> impl Iterator<Item = PitchClass> {
        (0..SEMITONES_PER_OCTAVE)
            .map(PitchClass::new)
            .filter(move |&c| self.contains(c))
    }
}

/// Modo de una tonalidad.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum KeyMode {
    /// Modo mayor.
    #[default]
    Major,
    /// Modo menor.
    Minor,
}

/// Armadura: número de alteraciones y modo.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct KeySignature {
    /// Alteraciones en el círculo de quintas, de -7 (siete bemoles) a 7 (siete sostenidos).
    pub fifths: i8,
    /// Mayor o menor.
    pub mode: KeyMode,
}

impl KeySignature {
    /// Tonalidad de do mayor.
    #[must_use]
    pub const fn c_major() -> Self {
        Self {
            fifths: 0,
            mode: KeyMode::Major,
        }
    }

    /// Clase de altura de la tónica.
    #[must_use]
    pub fn tonic(self) -> PitchClass {
        // Cada paso en el círculo de quintas son 7 semitonos.
        let major_tonic = PitchClass::new(0).transpose((i16::from(self.fifths) * 7 % 12) as i8);
        match self.mode {
            KeyMode::Major => major_tonic,
            // La menor relativa está 3 semitonos por debajo.
            KeyMode::Minor => major_tonic.transpose(-3),
        }
    }

    /// Conjunto de clases de altura de la escala diatónica.
    #[must_use]
    pub fn scale(self) -> PitchClassSet {
        // Intervalos de la escala mayor; para la menor natural se parte de la relativa.
        const MAJOR_STEPS: [i8; 7] = [0, 2, 4, 5, 7, 9, 11];
        let root = match self.mode {
            KeyMode::Major => self.tonic(),
            KeyMode::Minor => self.tonic().transpose(3),
        };
        let mut set = PitchClassSet::empty();
        for step in MAJOR_STEPS {
            set = set.with(root.transpose(step));
        }
        set
    }
}

/// Afinación de un instrumento de cuerda.
///
/// `midi_notes[0]` es la cuerda **más aguda** (la 1ª). Guardar el orden al revés es
/// el error clásico, así que los accesos van siempre por [`Tuning::string_pitch`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Tuning {
    /// Notas MIDI al aire, de la cuerda más aguda a la más grave.
    pub midi_notes: Vec<u8>,
    /// Nombre legible, por ejemplo `"Drop D"`. `None` si es una afinación sin nombre.
    pub label: Option<String>,
}

impl Default for Tuning {
    fn default() -> Self {
        Self::standard()
    }
}

impl Tuning {
    /// Afinación estándar de guitarra: E4 B3 G3 D3 A2 E2.
    #[must_use]
    pub fn standard() -> Self {
        Self {
            midi_notes: vec![64, 59, 55, 50, 45, 40],
            label: Some("Estándar".to_owned()),
        }
    }

    /// Afinación en Re grave: E4 B3 G3 D3 A2 D2.
    #[must_use]
    pub fn drop_d() -> Self {
        Self {
            midi_notes: vec![64, 59, 55, 50, 45, 38],
            label: Some("Drop D".to_owned()),
        }
    }

    /// Número de cuerdas.
    #[must_use]
    pub fn string_count(&self) -> u8 {
        // Una guitarra no pasa de 12 cuerdas; el truncamiento no puede ocurrir.
        self.midi_notes.len() as u8
    }

    /// Nota MIDI de una cuerda al aire, sin cejilla.
    ///
    /// `string` es 1 para la cuerda más aguda. Devuelve `None` si la cuerda no existe.
    #[must_use]
    pub fn string_pitch(&self, string: u8) -> Option<u8> {
        if string == 0 {
            return None;
        }
        self.midi_notes.get(usize::from(string - 1)).copied()
    }

    /// Altura real que suena al pisar un traste.
    ///
    /// El traste es relativo a la cejilla, así que la cejilla se suma aquí.
    #[must_use]
    pub fn sounding_pitch(&self, string: u8, fret: u8, capo: u8) -> Option<u8> {
        let open = self.string_pitch(string)?;
        open.checked_add(capo)?.checked_add(fret)
    }

    /// Todas las posiciones `(cuerda, traste)` que producen una altura dada.
    ///
    /// Es la base del solucionador de digitaciones: una misma nota se puede tocar en
    /// varios sitios del mástil, y elegir bien es lo que hace una tablatura cómoda.
    #[must_use]
    pub fn positions_for_pitch(&self, pitch: u8, capo: u8, fret_count: u8) -> Vec<(u8, u8)> {
        let mut positions = Vec::new();
        for string in 1..=self.string_count() {
            let Some(open) = self.string_pitch(string) else {
                continue;
            };
            let base = u16::from(open) + u16::from(capo);
            if u16::from(pitch) < base {
                continue;
            }
            let fret = u16::from(pitch) - base;
            if fret <= u16::from(fret_count) {
                positions.push((string, fret as u8));
            }
        }
        positions
    }
}

/// Convierte una nota MIDI a notación científica, por ejemplo `"E4"`.
#[must_use]
pub fn midi_to_scientific(midi: u8) -> String {
    let class = PitchClass::from_midi(midi);
    // En notación científica, do central (MIDI 60) es C4.
    let octave = i16::from(midi) / 12 - 1;
    format!("{}{octave}", class.name_sharp())
}

/// Interpreta notación científica, por ejemplo `"E4"` o `"Bb3"`, como nota MIDI.
#[must_use]
pub fn scientific_to_midi(text: &str) -> Option<u8> {
    let mut chars = text.chars();
    let letter = chars.next()?.to_ascii_uppercase();
    let base = match letter {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };

    let rest: String = chars.collect();
    let (accidental, octave_text) = match rest.chars().next() {
        Some('#') => (1_i16, &rest[1..]),
        Some('b') => (-1_i16, &rest[1..]),
        _ => (0_i16, rest.as_str()),
    };

    let octave: i16 = octave_text.parse().ok()?;
    let value = (octave + 1) * 12 + base + accidental;
    u8::try_from(value).ok()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{
        midi_to_scientific, scientific_to_midi, KeyMode, KeySignature, PitchClass, PitchClassSet,
        Tuning,
    };

    #[test]
    fn la_cuerda_uno_es_la_mas_aguda() {
        let tuning = Tuning::standard();
        assert_eq!(
            tuning.string_pitch(1),
            Some(64),
            "la 1ª cuerda es mi agudo (E4)"
        );
        assert_eq!(
            tuning.string_pitch(6),
            Some(40),
            "la 6ª cuerda es mi grave (E2)"
        );
        assert!(tuning.string_pitch(1).unwrap() > tuning.string_pitch(6).unwrap());
    }

    #[test]
    fn la_cuerda_cero_no_existe() {
        assert_eq!(Tuning::standard().string_pitch(0), None);
        assert_eq!(Tuning::standard().string_pitch(7), None);
    }

    #[test]
    fn la_cejilla_sube_la_altura_que_suena() {
        let tuning = Tuning::standard();
        // 6ª cuerda al aire sin cejilla: mi grave.
        assert_eq!(tuning.sounding_pitch(6, 0, 0), Some(40));
        // Con cejilla en el 3, la misma cuerda al aire suena un sol.
        assert_eq!(tuning.sounding_pitch(6, 0, 3), Some(43));
        // El traste es relativo a la cejilla y se suma encima.
        assert_eq!(tuning.sounding_pitch(6, 2, 3), Some(45));
    }

    #[test]
    fn una_altura_se_puede_tocar_en_varios_sitios() {
        let tuning = Tuning::standard();
        // Mi agudo (E4, MIDI 64): 1ª al aire, 2ª traste 5, 3ª traste 9...
        let positions = tuning.positions_for_pitch(64, 0, 22);
        assert!(positions.contains(&(1, 0)));
        assert!(positions.contains(&(2, 5)));
        assert!(positions.contains(&(3, 9)));
    }

    #[test]
    fn no_se_proponen_trastes_fuera_del_mastil() {
        let tuning = Tuning::standard();
        // Una nota muy aguda sólo cabe en las cuerdas agudas con pocos trastes.
        let positions = tuning.positions_for_pitch(88, 0, 22);
        assert!(positions.iter().all(|&(_, fret)| fret <= 22));
    }

    #[test]
    fn ida_y_vuelta_de_notacion_cientifica() {
        for midi in 21..=108_u8 {
            let text = midi_to_scientific(midi);
            assert_eq!(scientific_to_midi(&text), Some(midi), "falló con {text}");
        }
    }

    #[test]
    fn se_interpretan_los_bemoles() {
        assert_eq!(scientific_to_midi("Bb3"), Some(58));
        assert_eq!(scientific_to_midi("A#3"), Some(58));
        assert_eq!(scientific_to_midi("E4"), Some(64));
    }

    #[test]
    fn la_escala_de_do_mayor_no_tiene_alteraciones() {
        let scale = KeySignature::c_major().scale();
        for name in ["C", "D", "E", "F", "G", "A", "B"] {
            let class = scientific_to_midi(&format!("{name}4")).unwrap();
            assert!(
                scale.contains_midi(class),
                "{name} debería estar en do mayor"
            );
        }
        // Fa sostenido no pertenece a do mayor.
        assert!(!scale.contains_midi(scientific_to_midi("F#4").unwrap()));
    }

    #[test]
    fn sol_mayor_tiene_fa_sostenido() {
        let key = KeySignature {
            fifths: 1,
            mode: KeyMode::Major,
        };
        assert_eq!(
            key.tonic(),
            PitchClass::new(7),
            "la tónica de sol mayor es sol"
        );
        assert!(key
            .scale()
            .contains_midi(scientific_to_midi("F#4").unwrap()));
        assert!(!key.scale().contains_midi(scientific_to_midi("F4").unwrap()));
    }

    #[test]
    fn la_menor_comparte_notas_con_do_mayor() {
        let a_minor = KeySignature {
            fifths: 0,
            mode: KeyMode::Minor,
        };
        assert_eq!(
            a_minor.tonic(),
            PitchClass::new(9),
            "la tónica de la menor es la"
        );
        assert_eq!(a_minor.scale(), KeySignature::c_major().scale());
    }

    #[test]
    fn el_conjunto_de_alturas_transporta() {
        let set = PitchClassSet::from_classes(&[PitchClass::new(0), PitchClass::new(4)]);
        let up = set.transposed(2);
        assert!(up.contains(PitchClass::new(2)));
        assert!(up.contains(PitchClass::new(6)));
        assert_eq!(up.len(), 2);
    }
}
