# Rediseño del editor

## Objetivo

Rehacer el editor para que se sienta **curado y ordenado**. No sobra ninguna función (recortar,
cortar, reordenar bloques, mezclador de audio y extras del reproductor se quedan), pero hoy
todo convive en las mismas barras y el componente mide 2.181 líneas. El rediseño separa los
controles del vídeo, las herramientas de edición y la salida, y añade:

- Deshacer / rehacer.
- Selección de rango sobre la regla para quitar un tramo de una vez.
- Imanes al recortar y mover.
- Un menú de herramientas con los atajos (`atajo · acción`).
- **Exportación vertical 1080×1920** (TikTok, Shorts, Reels) con recorte por bloque o encajado
  con fondo desenfocado.

Referencia visual: SteelSeries Moments (vídeo grande arriba, timeline limpia abajo). Los iconos
actuales se mantienen de momento; se sustituirán más adelante.

## Alcance

- Frontend nuevo sobre el backend actual (export passthrough / GPU, `edits.rs`, mismo formato de
  edición guardada, ampliado de forma compatible).
- Backend: solo se amplía el camino de recodificación del export con la etapa de formato
  vertical.

Fuera de alcance: animación del encuadre (keyframes), más de dos pistas de audio, texto,
transiciones o efectos, y cualquier cambio en captura o replay.

---

## 1. Distribución de la pantalla

El editor sigue a pantalla completa. De arriba abajo:

1. **Cabecera** fina. Izquierda: clip anterior / siguiente y título. Derecha: deshacer, rehacer y
   cerrar. "Restablecer" deja la cabecera y pasa al menú de herramientas (destructivo y raro;
   además ahora se puede deshacer).
2. **Zona central**: visor + **panel "Formato" plegable a la derecha**, plegado por defecto (solo
   una pestaña con icono). Desplegado ofrece:
   - **Horizontal**: formato original; con solo cortes en keyframe exporta sin recodificar.
   - **Vertical 1080×1920**, modo *Recorte* o *Encajado*.
   En *Recorte* el visor muestra el marco 9:16 y se arrastra en horizontal; la posición es del
   bloque bajo el cabezal. En *Encajado* el visor muestra el resultado. El estado plegado se
   recuerda (preferencia local); el formato elegido se guarda con la edición del clip.
3. **Barra de reproducción** bajo el vídeo, solo para el vídeo: tiempo actual / duración a la
   izquierda; inicio, fotograma anterior, reproducir, fotograma siguiente, final en el centro;
   captura de fotograma y pantalla completa a la derecha.
4. **Timeline** en el panel inferior redimensionable: regla, pista de vídeo y las **dos pistas de
   audio siempre desplegadas**, cada una con cabecera alineada (nombre, silencio, volumen) y su
   forma de onda. Zoom con Ctrl+rueda.
5. **Barra de salida** al pie: botón de herramientas a la izquierda; marca de agua, **Compartir**
   y **Exportar** a la derecha, los tres con el mismo estilo. La duración final se lee en la barra
   de reproducción.

## 2. Modelo de edición

### Se conserva

Bloques (tramo de origen, posición en la timeline, límites máximos, desactivado). Los huecos
entre bloques se ven en negro y no se exportan. Se guarda en el mismo índice de `edits.rs`.

### Se añade

- `crop_x` por segmento: centro horizontal del marco 9:16, de 0 a 1, **0,5 por defecto**.
- `format` por clip: `horizontal` | `vertical` con `fill: "crop" | "fit"`. Por defecto
  horizontal.

Ambos son opcionales al leer: una edición guardada antes del rediseño carga centrada y en
horizontal.

### Operaciones (`edit-model.ts`, funciones puras)

| Operación | Comportamiento |
|---|---|
| Cortar | En el cabezal. Los dos bloques heredan `crop_x`. Cortar sobre un borde no crea bloques vacíos. |
| Recortar | Un borde, dentro de los límites del bloque, con imanes. |
| Mover / reordenar | Como hoy. |
| Quitar rango | Corta en ambos extremos, elimina lo de dentro y **cierra el hueco** desplazando lo siguiente a la izquierda. |
| Quitar bloque | Como hoy. |
| Desactivar / activar bloque | Como hoy. |
| Encuadre | Cambia `crop_x` del bloque. |
| Mezcla | Volumen y silencio por pista. |

### Imanes

Al recortar o mover, un borde se pega al cabezal, a los bordes de otros bloques y a inicio / fin
si está a **≤ 8 px en pantalla** (umbral en píxeles, convertido a ms con el zoom actual). Línea
vertical de guía mientras está pegado. **Alt** durante el arrastre los desactiva.

### Selección de rango

Arrastrar sobre la **regla** marca un tramo iluminado en todas las pistas; un clic sin arrastre
sigue moviendo el cabezal. Con rango: Supr lo quita, Escape lo descarta. Sin rango: Supr quita
el bloque seleccionado.

### Deshacer / rehacer (`edit-history.ts`)

- Guarda **copias completas** del estado editable (bloques, mezcla y formato), no operaciones
  inversas: listas pequeñas, copia barata, y deshacer vuelve exactamente al estado anterior.
- Un gesto continuo (arrastrar borde, mover bloque, deslizar volumen, arrastrar encuadre) es un
  solo paso, registrado al soltar.
- Límite de 100 pasos. Historial por sesión de clip; se vacía al cambiar de clip.

### Atajos

Tabla única (`shortcuts.ts`) usada por el teclado y por el menú de herramientas:

| Atajo | Acción |
|---|---|
| `Espacio` / `K` | Reproducir / pausa |
| `←` / `→` | Fotograma anterior / siguiente |
| `C` | Cortar en el cabezal |
| `Supr` | Quitar selección o bloque |
| `D` | Desactivar / activar bloque |
| `Ctrl+Z` | Deshacer |
| `Ctrl+Shift+Z` / `Ctrl+Y` | Rehacer |
| `F` | Pantalla completa |
| `Alt` (arrastrando) | Sin imanes |
| — | Restablecer montaje |

El menú de herramientas es un botón que abre la lista con **el atajo primero y luego la
acción**; cada fila también se puede pulsar.

## 3. Backend: formato vertical

Una etapa nueva en el **camino que recodifica** del export, entre el decodificador y la marca de
agua. La captura y el replay no se tocan.

- **Horizontal**: sin cambios (passthrough cuando solo hay cortes en keyframe).
- **Vertical**: siempre recodifica. Salida **1080×1920**.

### Composición (recorte y encajado)

Una etapa `Reframer` (`src-tauri/src/reframe.rs`) dibuja con **Direct2D sobre el device D3D11 del
decodificador**, igual que la marca de agua y el cartel de juego minimizado. El fotograma se copia
GPU→GPU a una textura propia (el decodificador entrega subtexturas de un array y Direct2D solo dibuja
desde la 0) y se dibuja sobre una textura 1080×1920 de un `IMFVideoSampleAllocatorEx`, que recicla
texturas cuando el encoder las suelta.

- **Recorte**: ventana 9:16 de altura completa centrada en `crop_x` del bloque, escalada a la salida.
- **Encajado**: fondo = recorte centrado a pantalla completa con el desenfoque gaussiano de Direct2D
  (desviación 3 % del ancho) y un velo al 35 %; delante, el fotograma entero escalado y centrado.

### Sin GPU

No hay camino por CPU: la captura ya exige D3D11 por hardware, así que un equipo sin GPU no tiene
clips propios que exportar. El export vertical devuelve un error claro.

### Recursos y calidad

- Texturas intermedias (copia del fotograma, fondo del encajado) reservadas una vez por export;
  las de salida las recicla el asignador. Ningún fotograma baja a memoria del sistema.
- La marca de agua se funde al final, sobre el lienzo de 1080×1920.
- El bitrate se calcula para el tamaño de salida.

### Compartir

El diálogo de compartir exporta en el formato elegido; los presets de tamaño siguen aplicando.

### Datos

`ClipEdit` gana `format` y cada segmento `crop_x`, ambos opcionales al deserializar.

## 4. Previsualización en el editor

- **Recorte**: el visor atenúa lo que queda fuera del marco 9:16; el marco se arrastra (gesto de
  un solo paso de deshacer).
- **Encajado**: el vídeo centrado; el fondo es un `<canvas>` diminuto al que se copia el
  fotograma en cada cuadro vía `requestVideoFrameCallback`, escalado y desenfocado por CSS. Sin
  segundo decodificador.

## 5. Organización del frontend

| Archivo | Responsabilidad |
|---|---|
| `components/editor/Editor.svelte` | Armazón: compone las piezas, atajos, cierre. Sin lógica de edición. |
| `components/editor/EditorHeader.svelte` | Navegación entre clips, título, deshacer / rehacer, cerrar. |
| `components/editor/Viewer.svelte` | `<video>`, previsualización de formato, pantalla completa. |
| `components/editor/Transport.svelte` | Controles de reproducción, tiempo, captura. |
| `components/editor/FormatPanel.svelte` | Panel plegable de formato. |
| `components/editor/Timeline.svelte` | Regla, zoom, cabezal, rango, imanes; coordina las pistas. |
| `components/editor/VideoTrack.svelte` | Bloques: recorte, movimiento, menú contextual. |
| `components/editor/AudioTrack.svelte` | Una pista de audio: cabecera y forma de onda. |
| `components/editor/OutputBar.svelte` | Herramientas, marca de agua, duración, compartir, exportar. |
| `components/editor/ToolsMenu.svelte` | Menú `atajo · acción` generado desde `shortcuts.ts`. |
| `lib/edit-model.ts` | Operaciones puras del modelo. |
| `lib/edit-history.ts` | Historial puro. |
| `lib/shortcuts.ts` | Tabla única de atajos. |
| `lib/editor.svelte.ts` | Estado reactivo del editor abierto e IPC (cargar, guardar, exportar). |

## 6. Pruebas

- **Vitest** (solo desarrollo) para `edit-model.ts` y `edit-history.ts`. Casos mínimos: cortar en
  un borde no crea bloques vacíos; quitar un rango que abarca varios bloques cierra el hueco;
  recortar respeta límites; imanes dentro de 8 px y no con Alt; un gesto largo es un paso;
  deshacer / rehacer vuelve al estado exacto; cortar conserva `crop_x`; edición antigua sin
  `crop_x` ni `format` carga centrada y horizontal.
- **Rust**: geometría del recorte y del encajado (rectángulos según `crop_x` y tamaño de origen,
  pares y dentro de los bordes) y deserialización de `ClipEdit` antiguos.
- Export completo (horizontal, recorte, encajado, con y sin marca de agua): prueba manual en la
  app.

## 7. Errores

- Si falla el export vertical se avisa como hoy; el original nunca se toca.
- Sin composición de dos flujos → shader; sin GPU → CPU. Transparente para el usuario.

## 8. Orden de construcción

1. `edit-model.ts` + `edit-history.ts` + `shortcuts.ts` con sus tests.
2. Editor nuevo reemplazando al actual con las funciones existentes (paridad).
3. Rango, imanes, deshacer / rehacer y menú de herramientas.
4. Formato vertical: datos, panel, previsualización y etapa del export.

Cada paso se prueba en la app antes del siguiente.
