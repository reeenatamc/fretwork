/**
 * Verifica el viaje de ida y vuelta del serializador AlphaTex.
 *
 * Genera los casos desde Rust, los parsea con el propio alphaTab, y compara lo que
 * alphaTab entiende con lo que quisimos escribir. Es la única comprobación honesta:
 * valida contra el programa que de verdad va a leer el formato, no contra nuestras
 * propias suposiciones.
 *
 * Uso:  node scripts/verify-alphatex.mjs [--dump]
 *       --dump  imprime el AlphaTex y el modelo interpretado de cada caso
 */
import { execFileSync } from 'node:child_process';
import * as alphaTab from '@coderline/alphatab';

const DUMP = process.argv.includes('--dump');

/** Ejecuta el generador de casos de Rust y devuelve el JSON. */
function loadFixtures() {
  const output = execFileSync(
    'cargo',
    ['run', '--quiet', '-p', 'tabs-core', '--bin', 'dump_fixtures'],
    { cwd: 'src-tauri', encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 },
  );
  return JSON.parse(output);
}

/** Parsea AlphaTex con alphaTab y devuelve la partitura junto con sus diagnósticos. */
function parseTex(tex) {
  const settings = new alphaTab.Settings();
  const importer = new alphaTab.importer.AlphaTexImporter();
  importer.logErrors = false;
  importer.initFromString(tex, settings);
  const score = importer.readScore();

  const diagnostics = [];
  for (const bag of [
    importer.lexerDiagnostics,
    importer.parserDiagnostics,
    importer.semanticDiagnostics,
  ]) {
    for (const item of bag?.diagnostics ?? []) {
      diagnostics.push(`${item.severity ?? '?'}: ${item.message ?? item}`);
    }
  }
  return { score, diagnostics };
}

/**
 * Proyecta la partitura de alphaTab al mismo formato que emite Rust.
 *
 * Hallazgo verificado con estos mismos casos: **el texto AlphaTex y el modelo interno de
 * alphaTab numeran las cuerdas al revés**. En el texto, `0.6` es la 6ª cuerda (mi grave),
 * igual que nuestra convención; en el modelo, `note.string === 1` es la más grave. De ahí
 * la conversión `stringCount + 1 - note.string`.
 *
 * Lo confirman los casos «melodía simple» y «con cejilla», donde las alturas resultantes
 * (`realValue`) coinciden con las calculadas por Rust.
 */
function project(score) {
  const track = score.tracks[0];
  const staff = track.staves[0];
  const stringCount = staff.tuning.length;

  return {
    title: score.title,
    tempo: score.tempo,
    // alphaTab guarda la afinación de aguda a grave, igual que nosotros.
    tuning: [...staff.tuning],
    capo: staff.capo ?? 0,
    bars: staff.bars.map((bar) => ({
      time_signature: [bar.masterBar.timeSignatureNumerator, bar.masterBar.timeSignatureDenominator],
      beats: (bar.voices[0]?.beats ?? []).map((beat) => ({
        duration: beat.duration,
        dots: beat.dots ?? 0,
        is_rest: beat.isRest,
        notes: [...beat.notes]
          .map((note) => ({
            // Se anota el valor crudo para poder diagnosticar si la convención difiere.
            rawString: note.string,
            string: stringCount + 1 - note.string,
            fret: note.fret,
            sounding_midi: note.realValue,
          }))
          .sort((a, b) => a.string - b.string),
      })),
    })),
  };
}

/** Compara dos valores y acumula las diferencias con su ruta. */
function diff(path, expected, actual, out) {
  if (Array.isArray(expected)) {
    if (!Array.isArray(actual)) {
      out.push(`${path}: se esperaba una lista, llegó ${typeof actual}`);
      return;
    }
    if (expected.length !== actual.length) {
      out.push(`${path}: se esperaban ${expected.length} elementos, llegaron ${actual.length}`);
      return;
    }
    expected.forEach((item, index) => diff(`${path}[${index}]`, item, actual[index], out));
    return;
  }

  if (expected !== null && typeof expected === 'object') {
    for (const key of Object.keys(expected)) {
      diff(`${path}.${key}`, expected[key], actual?.[key], out);
    }
    return;
  }

  if (typeof expected === 'number' && typeof actual === 'number') {
    if (Math.abs(expected - actual) > 0.001) {
      out.push(`${path}: se esperaba ${expected}, llegó ${actual}`);
    }
    return;
  }

  if (expected !== actual) {
    out.push(`${path}: se esperaba ${JSON.stringify(expected)}, llegó ${JSON.stringify(actual)}`);
  }
}

function main() {
  const fixtures = loadFixtures();
  let failed = 0;

  for (const { name, tex, expected } of fixtures) {
    let actual;
    let diagnostics = [];

    try {
      const parsed = parseTex(tex);
      diagnostics = parsed.diagnostics;
      actual = project(parsed.score);
    } catch (error) {
      console.log(`✗ ${name}\n    alphaTab no pudo parsear: ${error.message}`);
      console.log(indent(tex));
      failed += 1;
      continue;
    }

    // Se comparan sólo las claves que Rust declara; las extra de alphaTab se ignoran.
    const problems = [];
    diff('', expected, actual, problems);

    if (DUMP) {
      console.log(`\n─── ${name} ───\n${indent(tex)}`);
      console.log(indent(JSON.stringify(actual, null, 2)));
    }

    if (problems.length === 0 && diagnostics.length === 0) {
      console.log(`✓ ${name}`);
    } else {
      failed += 1;
      console.log(`✗ ${name}`);
      for (const d of diagnostics) console.log(`    diagnóstico: ${d}`);
      for (const p of problems.slice(0, 12)) console.log(`    ${p}`);
      if (problems.length > 12) console.log(`    … y ${problems.length - 12} diferencias más`);
      if (!DUMP) console.log(indent(tex));
    }
  }

  console.log(`\n${fixtures.length - failed}/${fixtures.length} casos correctos`);
  process.exit(failed === 0 ? 0 : 1);
}

function indent(text) {
  return text
    .split('\n')
    .map((line) => `    ${line}`)
    .join('\n');
}

main();
