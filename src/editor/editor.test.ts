/**
 * Pruebas del cursor y del mapeo de teclas.
 *
 * Son funciones puras a propósito: se pueden probar sin navegador, sin alphaTab y sin
 * partitura. Se ejecutan con `node --test`.
 */
import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import { extractVideoId } from '../player/youtube';
import {
  type CursorBounds,
  createCursor,
  moveBeat,
  moveString,
  moveToBar,
  moveToBeat,
  toAddr,
} from './cursor';
import { DURATIONS, FretAccumulator, interpret, stepDuration } from './keymap';

/** Compases de 4 pulsos y guitarra de 6 cuerdas. */
const bounds: CursorBounds = {
  barCount: 4,
  stringCount: 6,
  beatsPerBar: () => 4,
};

/** Construye un evento de teclado sin necesitar un navegador. */
function key(init: Partial<KeyboardEvent> & { key: string }): KeyboardEvent {
  return {
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    altKey: false,
    ...init,
  } as KeyboardEvent;
}

describe('cursor', () => {
  it('arranca en el primer compás, primer pulso y primera cuerda', () => {
    const cursor = createCursor();
    assert.deepEqual(cursor, { bar: 0, beat: 0, string: 1, voice: 0 });
  });

  it('la flecha arriba va hacia la cuerda aguda', () => {
    const cursor = { bar: 0, beat: 0, string: 3, voice: 0 };
    assert.equal(moveString(cursor, -1, bounds).string, 2);
    assert.equal(moveString(cursor, 1, bounds).string, 4);
  });

  it('no se sale por los extremos del diapasón', () => {
    // Saltar de la prima al bordón sin avisar desorienta al transcribir.
    assert.equal(moveString({ bar: 0, beat: 0, string: 1, voice: 0 }, -1, bounds).string, 1);
    assert.equal(moveString({ bar: 0, beat: 0, string: 6, voice: 0 }, 1, bounds).string, 6);
  });

  it('avanzar al final del compás pasa al siguiente', () => {
    const cursor = { bar: 0, beat: 3, string: 1, voice: 0 };
    const next = moveBeat(cursor, 1, bounds);
    assert.equal(next.bar, 1);
    assert.equal(next.beat, 0);
  });

  it('retroceder al principio del compás vuelve al anterior por su último pulso', () => {
    const cursor = { bar: 2, beat: 0, string: 1, voice: 0 };
    const previous = moveBeat(cursor, -1, bounds);
    assert.equal(previous.bar, 1);
    assert.equal(previous.beat, 3);
  });

  it('no retrocede más allá del principio de la pieza', () => {
    const cursor = { bar: 0, beat: 0, string: 1, voice: 0 };
    assert.deepEqual(moveBeat(cursor, -1, bounds), cursor);
  });

  it('en el último compás se puede seguir escribiendo hacia delante', () => {
    // Si el cursor se plantara en el último pulso, no habría forma de alargar la canción.
    const cursor = { bar: 3, beat: 3, string: 1, voice: 0 };
    const next = moveBeat(cursor, 1, bounds);
    assert.equal(next.bar, 3);
    assert.equal(next.beat, 4);
  });

  it('saltar a un compás fuera de rango se queda en el borde', () => {
    assert.equal(moveToBar(createCursor(), 99, bounds).bar, 3);
    assert.equal(moveToBar(createCursor(), -5, bounds).bar, 0);
  });

  it('seguir a la reproducción planta el cursor en el pulso que suena', () => {
    const cursor = { bar: 0, beat: 0, string: 3, voice: 0 };
    const following = moveToBeat(cursor, 2, 1, bounds);
    assert.deepEqual(following, { bar: 2, beat: 1, string: 3, voice: 0 });
  });

  it('un pulso que ya no existe no saca el cursor de la partitura', () => {
    // Lo que suena puede ser una versión anterior a la que se está editando.
    assert.equal(moveToBeat(createCursor(), 40, 0, bounds).bar, 3);
    assert.equal(moveToBeat(createCursor(), -1, -1, bounds).bar, 0);
    assert.equal(moveToBeat(createCursor(), -1, -1, bounds).beat, 0);
  });

  it('la dirección para Rust lleva pista, pentagrama y voz', () => {
    const addr = toAddr({ bar: 2, beat: 1, string: 4, voice: 0 });
    assert.deepEqual(addr, { track: 0, staff: 0, bar: 2, voice: 0, beat: 1 });
  });
});

describe('mapeo de teclas', () => {
  it('las flechas mueven', () => {
    assert.deepEqual(interpret(key({ key: 'ArrowUp' })), { type: 'moveString', delta: -1 });
    assert.deepEqual(interpret(key({ key: 'ArrowRight' })), { type: 'moveBeat', delta: 1 });
  });

  it('los dígitos son trastes', () => {
    assert.deepEqual(interpret(key({ key: '7' })), { type: 'digit', value: 7 });
    assert.deepEqual(interpret(key({ key: '0' })), { type: 'digit', value: 0 });
  });

  it('el espacio avanza', () => {
    assert.deepEqual(interpret(key({ key: ' ' })), { type: 'advance' });
  });

  it('borrar limpia la cuerda y con mayúsculas quita el pulso entero', () => {
    assert.deepEqual(interpret(key({ key: 'Backspace' })), { type: 'clearString' });
    assert.deepEqual(interpret(key({ key: 'Backspace', shiftKey: true })), { type: 'removeBeat' });
  });

  it('deshacer y rehacer', () => {
    assert.deepEqual(interpret(key({ key: 'z', ctrlKey: true })), { type: 'undo' });
    assert.deepEqual(interpret(key({ key: 'z', ctrlKey: true, shiftKey: true })), { type: 'redo' });
    assert.deepEqual(interpret(key({ key: 'y', ctrlKey: true })), { type: 'redo' });
  });

  it('las letras activan técnicas', () => {
    const hammer = interpret(key({ key: 'h' }));
    assert.equal(hammer?.type, 'toggleTechnique');
    const vibrato = interpret(key({ key: 'v' }));
    assert.equal(vibrato?.type, 'toggleTechnique');
    assert.notDeepEqual(hammer, vibrato, 'cada letra es una técnica distinta');
  });

  it('las teclas desconocidas se dejan pasar', () => {
    // Devolver null es lo que impide romper atajos del sistema.
    assert.equal(interpret(key({ key: 'F5' })), null);
    assert.equal(interpret(key({ key: 'q', altKey: true })), null);
  });
});

describe('acumulador de trastes', () => {
  it('un dígito suelto es el traste', () => {
    const frets = new FretAccumulator();
    assert.equal(frets.push(5, 1000), 5);
  });

  it('dos dígitos seguidos forman un traste de dos cifras', () => {
    // Sin esto no se podría escribir el traste 12: el 1 ya habría confirmado la nota.
    const frets = new FretAccumulator();
    assert.equal(frets.push(1, 1000), 1);
    assert.equal(frets.push(2, 1300), 12);
  });

  it('pasada la ventana empieza una cifra nueva', () => {
    const frets = new FretAccumulator(900);
    assert.equal(frets.push(1, 1000), 1);
    assert.equal(frets.push(2, 5000), 2, 'demasiado tarde para combinar');
  });

  it('no se combina si el resultado no cabe en el mástil', () => {
    const frets = new FretAccumulator(900, 24);
    assert.equal(frets.push(9, 1000), 9);
    assert.equal(frets.push(9, 1100), 9, '99 no existe, así que es un 9 nuevo');
  });

  it('mover el cursor olvida el dígito pendiente', () => {
    const frets = new FretAccumulator();
    assert.equal(frets.push(1, 1000), 1);
    frets.reset();
    assert.equal(frets.push(2, 1100), 2, 'ya no se combina con el 1');
  });

  it('el cero no abre una cifra de dos', () => {
    const frets = new FretAccumulator();
    assert.equal(frets.push(0, 1000), 0);
    assert.equal(frets.push(5, 1100), 5, 'no sale 5 combinado con el cero');
  });
});

describe('teclas del reproductor', () => {
  it('Intro suena y Mayúsculas+Intro repite el compás', () => {
    assert.deepEqual(interpret(key({ key: 'Enter' })), { type: 'play' });
    assert.deepEqual(interpret(key({ key: 'Enter', shiftKey: true })), { type: 'loopBar' });
  });

  it('la eme enciende el metrónomo', () => {
    assert.deepEqual(interpret(key({ key: 'm' })), { type: 'toggleMetronome' });
    assert.deepEqual(interpret(key({ key: 'M' })), { type: 'toggleMetronome' });
  });
});

describe('enlaces de YouTube', () => {
  it('reconoce las formas que uno copia del navegador', () => {
    // Todas apuntan al mismo vídeo; da igual de dónde se copie el enlace.
    const id = 'Man4Xw8Xypo';
    assert.equal(extractVideoId(`https://www.youtube.com/watch?v=${id}`), id);
    assert.equal(extractVideoId(`https://youtu.be/${id}`), id);
    assert.equal(extractVideoId(`https://www.youtube.com/embed/${id}`), id);
    assert.equal(extractVideoId(`https://www.youtube.com/shorts/${id}`), id);
  });

  it('aguanta los parámetros de sobra que traen pegados', () => {
    assert.equal(
      extractVideoId('https://www.youtube.com/watch?v=Man4Xw8Xypo&list=RD&index=2&t=42s'),
      'Man4Xw8Xypo',
    );
    assert.equal(extractVideoId('https://youtu.be/Man4Xw8Xypo?si=abc123'), 'Man4Xw8Xypo');
  });

  it('acepta un identificador suelto', () => {
    assert.equal(extractVideoId('Man4Xw8Xypo'), 'Man4Xw8Xypo');
    assert.equal(extractVideoId('  Man4Xw8Xypo  '), 'Man4Xw8Xypo');
  });

  it('devuelve null si no hay nada reconocible', () => {
    assert.equal(extractVideoId(''), null);
    assert.equal(extractVideoId('https://ejemplo.com/video'), null);
    assert.equal(extractVideoId('corto'), null);
  });
});

describe('figuras rítmicas', () => {
  it('se acortan y se alargan de una en una', () => {
    assert.equal(stepDuration(4, 'shorter'), 8);
    assert.equal(stepDuration(4, 'longer'), 2);
  });

  it('no se pasan de los extremos', () => {
    assert.equal(stepDuration(DURATIONS[0], 'longer'), DURATIONS[0]);
    assert.equal(stepDuration(DURATIONS.at(-1) as number, 'shorter'), DURATIONS.at(-1));
  });
});
