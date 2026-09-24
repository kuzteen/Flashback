# Muxer MP4 propio (híbrido y progresivo)

## Objetivo

Que una grabación manual **sobreviva a un cierre inesperado** (crash, apagón, pantallazo azul) y
que los replays guardados tengan el índice al principio de verdad.

Hoy todo lo que se escribe en passthrough (grabación manual vía `LiveMux`, guardado de replay vía
`mux_replay`) pasa por el `IMFSinkWriter` de Media Foundation, que ignora
`MF_MPEG4SINK_MOOV_BEFORE_MDAT` y deja el `moov` al final (medido: `ftyp, uuid, mdat, moov`). Si la
app muere a mitad de una grabación, el `mdat` queda sin índice y el archivo no se puede reproducir.

La solución es un muxer MP4 propio en Rust puro con dos modos:

- **Híbrido** (grabación manual): MP4 fragmentado mientras se graba; al parar se convierte en un
  MP4 estándar sin reescribir los datos. Es el enfoque de OBS 30.2 ("Hybrid MP4").
- **Progresivo** (guardado de replay): `ftyp → moov → mdat`, con el índice delante.

## Alcance

- Módulo nuevo `src-tauri/src/mp4mux/`, sin dependencias de Windows ni de Media Foundation.
- `LiveMux` deja de usar `IMFSinkWriter` y escribe con el modo híbrido desde un hilo propio.
- `mux_replay` se sustituye por el modo progresivo.
- `library::mp4_duration_secs` aprende a leer la duración de un MP4 fragmentado.

Fuera de alcance:

- La exportación del editor (`editor.rs`) sigue con Media Foundation. Se revisará en el trabajo de
  exportación MOV.
- Reparar al arrancar un archivo que quedó fragmentado tras un cierre inesperado. Ya es
  reproducible tal cual; no se reescribe.
- Otros contenedores (MKV, MOV) y otros códecs de vídeo (HEVC, AV1).

---

## 1. Módulo `mp4mux`

Rust puro, testeable sin Windows. Entrada: paquetes ya codificados, igual que los recibe hoy
`LiveMux`.

- **Vídeo:** H.264 en Annex B (NALs separados por códigos de inicio `00 00 01` / `00 00 00 01`),
  con tiempo de presentación y duración en unidades de 100 ns y marca de keyframe. El encoder va
  sin B-frames, así que el orden de decodificación es el de presentación (sin `ctts`).
- **Cabecera de secuencia de vídeo:** SPS/PPS en Annex B (lo que hoy llega por `set_seq_header`).
- **Audio:** hasta dos pistas AAC crudas (payload 0) con tiempo y duración en 100 ns.
- **Cabecera de audio:** el `MF_MT_USER_DATA` del encoder AAC: 12 bytes de `HEAACWAVEINFO`
  seguidos del AudioSpecificConfig. Si el blob mide 12 bytes o menos se considera inválido y la
  pista se descarta, igual que hoy.

Submódulos:

- `annexb`: separa los NAL de un paquete Annex B. Calcula el tamaño en formato MP4 (4 bytes de
  longitud + NAL) sin copiar y escribe NAL a NAL. Descarta AUD (9), SPS (7) y PPS (8) en banda,
  que ya van en `avcC`. Construye `avcC` a partir del SPS/PPS (perfil, compatibilidad y nivel
  salen de los bytes 1–3 del SPS).
- `aac`: extrae el AudioSpecificConfig del user data y construye `esds` y la entrada `mp4a`.
- `boxes`: escritor de cajas ISO BMFF (tamaño a posteriori, `largesize` cuando hace falta).
- `table`: tabla de muestras por pista (tamaño, duración, desplazamiento de fragmento, keyframe) y
  generación de `stts`, `stsz`, `stsc`, `stco`/`co64`, `stss` y `edts`/`elst`.
- `hybrid`: el escritor híbrido (sección 2).
- `progressive`: el escritor progresivo (sección 3).

### Tiempos

- Escala de tiempo del vídeo: 90 000. Las duraciones se derivan de los tiempos absolutos
  redondeados (`round(t[i+1]) - round(t[i])`) para que el redondeo no acumule deriva a ninguna
  tasa (60, 144, 165, 240…).
- Escala de tiempo de cada pista de audio: su frecuencia de muestreo.
- `mvhd` a escala 1000.
- El origen de tiempos es el primer keyframe de vídeo (lo fija `LiveMux`, como hoy). Si una
  pista de audio empieza más tarde (silencio inicial del loopback), el `moov` final lleva una
  lista de edición con un tramo vacío del hueco. En los fragmentos, el `tfdt` de cada pista ya
  refleja su tiempo real.

### Memoria

La tabla guarda ~16 bytes por muestra: una hora a 60 fps con dos pistas de audio son ~9 MB.

## 2. Modo híbrido (grabación manual)

Disposición mientras se graba:

```
ftyp  moov(mvhd, trak×N, mvex(trex×N))  [moof mdat]  [moof mdat]  …
```

- Un **fragmento por keyframe de vídeo** (~1 s con el GOP actual). Cada fragmento lleva un `moof`
  con un `traf` por pista (`tfhd`, `tfdt`, `trun`) y un `mdat` con el vídeo del GOP seguido del
  audio de ese tramo. El audio se agrupa en el fragmento cuyo intervalo contiene su tiempo; el
  audio que llega con retraso respecto al vídeo espera al siguiente fragmento.
- Tras cada fragmento se vacía el buffer de escritura al sistema operativo. Un cierre de la app
  pierde como mucho el fragmento en curso. No se llama a `FlushFileBuffers`: ante un corte de luz
  puede perderse además lo que la caché del sistema aún no ha escrito (segundos).

Al finalizar (parar la grabación):

1. Se vuelca el fragmento pendiente.
2. Se añade al final un `moov` **completo, sin `mvex`**, cuyas tablas apuntan directamente a los
   datos dentro de cada `mdat` (`co64` si algún desplazamiento pasa de 4 GB).
3. Se cambia el tipo del `moov` inicial a `free`.
4. Se cambia el tipo de cada `moof` a `free` (se guardan sus desplazamientos; 4 bytes por
   fragmento).

Resultado: `ftyp free [free mdat]… moov`, un MP4 estándar no fragmentado. No se reescribe ningún
dato y cuesta milisegundos. El orden de los pasos protege el cierre: si la app muere entre el 2 y
el 3, el primer `moov` sigue siendo el fragmentado y el archivo se reproduce como tal.

Si una escritura falla (disco lleno), el escritor deja de aceptar paquetes, recorta el archivo al
final del último fragmento completo (así no queda un fragmento a medias) y lo conserva: es
reproducible. Solo se borra el archivo
si no llegó a escribirse ningún fragmento.

## 3. Modo progresivo (guardado de replay)

Todos los paquetes están ya en RAM. Se calcula el tamaño MP4 de cada muestra sin copiar
(`annexb`), se construye el `moov` con desplazamientos definitivos y se escribe
`ftyp → moov → mdat`, convirtiendo NAL a NAL al escribir. No duplica el replay en memoria.

Mismo criterio de audio que hoy: se descartan paquetes anteriores al origen de tiempos y la
duración del último vídeo es la nominal de un fotograma.

## 4. Integración con la captura

- `LiveMux` conserva su lógica: espera a tener las cabeceras (con el mismo timeout de 1 s que
  descarta pistas mudas), fija la base en el primer keyframe, desplaza los tiempos tras un
  rebuild (`new_segment`) y `recording_segment` abre un archivo nuevo si cambia el tamaño.
- Solo cambia el destino: en vez de `IMFSinkWriter`, `LiveMux` envía los paquetes por un canal a
  un hilo `flashback-mux`, dueño del archivo y del escritor híbrido. Los paquetes viajan como el
  mismo `Arc<Vec<u8>>` que comparte el ring buffer: **cero copias** en el hilo del encoder, que
  hoy copia cada paquete a un `MFCreateMemoryBuffer` y llama a `WriteSample` con el lock del ring
  tomado.
- El canal no tiene límite: si el disco se atasca crece la RAM, pero nunca se bloquea la captura
  (mismo comportamiento que el `IMFSinkWriter` actual con `MF_SINK_WRITER_DISABLE_THROTTLING`).
- `finalize()` manda la orden de cierre, espera al hilo y devuelve la ruta si el archivo es
  válido. La semántica de `finish_mux` se ajusta: solo borra el archivo si no llegó a escribirse
  ningún fragmento.
- `save_replay` llama al modo progresivo desde su hilo de muxado, como hoy.
- Se retiran `add_h264_passthrough_stream`, `add_aac_passthrough_stream` y `write_audio_until`,
  y los comentarios que prometían faststart.

## 5. Duración de la biblioteca

`mp4_duration_secs` sigue leyendo `mvhd` del primer `moov`. Si ese `moov` tiene `mvex` (archivo
fragmentado que quedó abierto) y la duración es 0, recorre los `moof` y toma la del vídeo
(pista 1): `tfdt` del último fragmento + suma de duraciones de su `trun`.

## 6. Pruebas

- **Tests unitarios** (`cargo test`): separación de NAL con los dos tipos de código de inicio,
  cálculo de tamaños y conversión, `avcC` desde SPS/PPS, `esds` desde user data, tablas
  (`stts` con duraciones variables, `stsc`, cambio a `co64`), lista de edición con audio
  retrasado. Un lector mínimo de cajas en los tests verifica la estructura del archivo en los
  dos modos, incluidos los `free` tras finalizar y un archivo híbrido sin finalizar.
- **Integración con un fixture real:** un H.264 Annex B y unos frames AAC diminutos (generados
  una sola vez con ffmpeg y versionados en `src-tauri/tests/fixtures/`). El test escribe el MP4
  en los dos modos, y también un híbrido sin finalizar, y lo abre con `IMFSourceReader` contando
  muestras de vídeo y audio.
- **Manual en la app:**
  - Grabación real revisada con `ffprobe -v error`.
  - Reproducción en la biblioteca.
  - Exportación passthrough en el editor.
  - Replay guardado con `moov` delante.
  - **Matar el proceso a mitad de grabación** y comprobar que el archivo se reproduce y que la
    biblioteca muestra su duración.

## 7. Documentación

Actualizar en `CLAUDE.md` el párrafo de "Multiplexado en vivo" y el mapa de módulos: `LiveMux`
escribe con `mp4mux` (MP4 híbrido: fragmentado mientras graba, estándar al parar) y el replay se
guarda con el índice delante.
