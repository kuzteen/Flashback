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
5. **Barra de salida** al pie: botón de herramientas a la izquierda; marca de agua, duración
   final, **Compartir** y **Exportar** (primario) a la derecha.

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

### Recorte

Ventana 9:16 de altura completa del origen (608×1080 en un 1080p), centrada en `crop_x` del
bloque y limitada a los bordes, escalada a 1080×1920 en **una pasada del procesador de vídeo de
D3D11**. El export ya avanza bloque a bloque, así que el rectángulo de origen solo cambia al
empezar cada bloque. Nota: desde 1080p el recorte se amplía ≈1,8× y pierde nitidez; desde 1440p
queda casi nativo.

### Encajado con fondo desenfocado

- Fondo: el fotograma se reduce a una miniatura (≈54×96) y se amplía a 1080×1920; el filtrado
  del escalado produce el desenfoque. Pasadas del procesador de vídeo, en GPU.
- Delante: el fotograma completo escalado a 1080×608, centrado.
- Composición: dos flujos en una pasada si la GPU lo admite; si no, un pixel shader sobre el
  mismo device.
- `crop_x` no se usa en este modo.

### Recursos y calidad

- Texturas intermedias (miniatura, fondo, salida) reservadas una vez por export y reutilizadas
  cada fotograma. Ningún fotograma baja a memoria del sistema.
- La marca de agua se funde al final, sobre el lienzo de 1080×1920.
- El bitrate se calcula para el tamaño de salida.

### Sin GPU

El camino por CPU existente produce el mismo resultado, más lento (el desenfoque también parte
de la miniatura, barata en CPU). Es export, no captura.

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
