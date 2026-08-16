//! Núcleo del modelo de tablaturas.
//!
//! Contiene el modelo canónico de partitura ([`model`]), las utilidades de altura y
//! diapasón ([`pitch`]) y la serialización a AlphaTex ([`alphatex`]), que es el formato
//! que alphaTab sabe leer para renderizar y sonar.
//!
//! El almacenamiento es JSON: AlphaTex sólo es transporte hacia el renderizador, porque
//! alphaTab no tiene exportador de AlphaTex y el formato no sabe expresar los
//! identificadores internos ni la procedencia de los arreglos.

pub mod alphatex;
pub mod difficulty;
pub mod edit;
pub mod model;
pub mod pitch;
pub mod transform;

pub use model::{Score, SCHEMA_VERSION};
