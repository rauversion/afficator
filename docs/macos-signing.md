# Firma y notarizacion en macOS

Rau Studio puede compilar bundles para macOS sin certificado, pero esos builds no pasan Gatekeeper para usuarios finales. La firma ad-hoc evita bundles rotos en Apple Silicon, pero no reemplaza la firma con **Developer ID Application** ni la notarizacion de Apple.

## Estado actual

- Los artefactos macOS publicados son `.app.tar.gz`.
- No estan firmados con Developer ID.
- No estan notarizados.
- Para abrirlos en una maquina local hay que quitar `com.apple.quarantine`.

```sh
cd ~/Downloads
tar -xzf RauStudio_0.1.7_arm64.app.tar.gz
xattr -dr com.apple.quarantine "Rau Studio.app"
open "Rau Studio.app"
```

## Para distribuir sin aviso de Gatekeeper

Requisitos:

- Cuenta pagada de Apple Developer.
- Certificado **Developer ID Application**.
- Certificado exportado desde Keychain como `.p12`.
- App-specific password del Apple ID o credenciales de App Store Connect.
- Team ID de Apple Developer.

Secrets recomendados para GitHub Actions:

```text
APPLE_CERTIFICATE          # contenido base64 del .p12
APPLE_CERTIFICATE_PASSWORD # password usado al exportar el .p12
KEYCHAIN_PASSWORD          # password temporal para el keychain del runner
APPLE_ID                   # email del Apple ID
APPLE_PASSWORD             # app-specific password
APPLE_TEAM_ID              # Team ID de Apple Developer
APPLE_SIGNING_IDENTITY     # opcional: "Developer ID Application: Nombre (TEAMID)"
```

Para generar `APPLE_CERTIFICATE`:

```sh
openssl base64 -A -in DeveloperIDApplication.p12 -out apple_certificate_base64.txt
```

## Flujo recomendado

1. Generar un certificado **Developer ID Application** desde Apple Developer.
2. Exportarlo desde Keychain como `.p12`.
3. Guardar el `.p12` como base64 en `APPLE_CERTIFICATE`.
4. Configurar los secrets de Apple en GitHub.
5. Cambiar el workflow de release para firmar con `APPLE_SIGNING_IDENTITY`.
6. Notarizar el bundle con Apple antes de publicarlo.

Cuando esto quede activo, el artefacto recomendado para usuarios finales deberia ser un `.dmg` firmado y notarizado.

## Servicios que simplifican el proceso

- **Codemagic**: simplifica code signing y App Store Connect para macOS.
- **CrabNebula Cloud**: buena opcion para apps Tauri, sobre todo si despues se necesita updater/distribucion.
- **GitHub Actions**: suficiente para este proyecto si los secrets quedan bien configurados.

Ningun servicio serio evita la cuenta Apple Developer ni el certificado Developer ID; solo automatizan el manejo de credenciales, firma, notarizacion y publicacion.
