# Importar XML generado en Rekordbox

Esta guia resume solo la parte de importacion del HTML adjunto: como abrir en Rekordbox el XML exportado por Aifficator y traer sus playlists a tu libreria.

El punto clave: el XML no contiene audio embebido. Rekordbox lee las rutas `Location` del XML, por eso los archivos originales y las carpetas `converted/` deben seguir en el mismo lugar.

## 1. Activar el panel de rekordbox xml

La funcion `rekordbox xml` no siempre viene visible por defecto.

En Rekordbox, abre:

```text
Preferences > View > Layout
```

Activa el checkbox `rekordbox xml`.

![Activar panel rekordbox xml](./rekordbox-xml-display.png)

## 2. Seleccionar el XML exportado por Aifficator

Sin salir de preferencias, abre:

```text
Advanced > Database
```

En la seccion `Imported Library`, pulsa `Browse` y selecciona el XML que exportaste desde Aifficator.

![Seleccionar Imported Library](./rekordbox-xml-library.png)

Cuando Rekordbox pida el archivo, elige el XML exportado y pulsa `Open`. Despues puedes cerrar la ventana de preferencias.

![Abrir archivo XML](./rekordbox-import-xml-file.png)

## 3. Abrir la libreria XML dentro de Rekordbox

En el browser de Rekordbox, abre la categoria `rekordbox xml`.

Ahi deberias ver:

- `All Tracks`: todos los tracks incluidos en el XML.
- `Playlists`: las playlists exportadas.

![Panel rekordbox xml](./rekordbox-xml-library-tab.png)

Si vuelves a exportar el XML desde Aifficator, usa el boton de refrescar junto a `All Tracks` para recargarlo sin configurar todo otra vez.

![Icono refrescar XML](./rekordbox-refresh-icon.png)

## 4. Importar playlists o tracks

Para importar una playlist:

1. Abre `rekordbox xml`.
2. Abre `Playlists`.
3. Haz click derecho sobre la playlist.
4. Elige `Import Playlist`.

Rekordbox creara la playlist dentro de tu libreria y analizara los tracks si lo necesita.

Tambien puedes importar tracks sueltos con click derecho y `Import To Collection`.

## Notas para Aifficator

- El XML exportado por Aifficator mantiene la coleccion completa.
- Los tracks no convertidos conservan su `Location` original.
- Los tracks convertidos apuntan a `converted/*.aiff`.
- Si mueves la musica o borras una carpeta `converted/`, Rekordbox puede mostrar archivos faltantes.
- Para probar el flujo, conviene importar primero una playlist chica.

Referencia original del HTML adjunto: `https://www.djuced.com/kb/djuced-rekordbox-xml/`.
