# Web Traffic

Analítica web respetuosa con la privacidad dentro de Rustrak: quién visita tu
web, de dónde viene y qué lee, junto a los errores que se encuentra. Sin
cookies, sin banner de consentimiento, sin terceros.

## Qué incluye

Filtros en todas las páginas (página, referencia, campaña, país,
navegador…), que se aplican con un clic en cualquier valor, y vistas
guardadas como en el resto de Rustrak.

- **Visitantes, páginas vistas y tasa de rebote**, frente al periodo anterior.
- **En línea ahora**: visitantes de los últimos cinco minutos.
- **Páginas, referencias, fuentes y campañas** más vistas (`utm_*` y `ref=`).
- **Países, navegadores, sistemas operativos y dispositivos.**
- **Tus propios eventos**: un registro, una compra.
- **Tráfico en la página de un issue**: en cada issue, cuánta gente vio la
  página donde ocurre, para saber cuánto pesa el error.
- **Últimas 24 horas** al final del resumen de cada proyecto.
- **Páginas de entrada y salida**, y cualquier lista completa con su tasa de
  rebote y duración de la visita.
- **Visitantes**: el día de cada uno, visita a visita, y de dónde llegó.
- **Eventos**: en el tiempo, sus propiedades y las páginas donde ocurren.

## Instalación

1. En **Configuración › Plugins › Web Traffic › Configuración**, actívalo en
   un proyecto.
2. Abre ese proyecto ahí: su **Instalación** tiene el script con la clave del
   proyecto. Pégalo en el `<head>` de todas las páginas:

```html
<script defer src="https://rustrak.example.com/public/web-traffic/script.js"
  data-project="1" data-key="<la clave pública del DSN del proyecto>"></script>
```

Las aplicaciones de una sola página se siguen con la History API. Para contar
un evento:

```js
webTraffic.track('signup', { plan: 'pro' });
```

```html
<button data-traffic-event="buy">Comprar</button>
```

## Ajustes

| Ajuste | Dónde | Qué hace |
| --- | --- | --- |
| Dominios | proyecto | Cuenta solo estos sitios (con sus subdominios). Vacío cuenta cualquiera. |
| Guardar la query | proyecto | Guarda `?busqueda=…` con cada página. |
| Respetar Do Not Track | instancia | No cuenta a quien pide en su navegador no ser rastreado. Activado por defecto. |

## Privacidad

Un visitante es un hash de su dirección, su navegador y una sal que se
cambia cada día. La dirección nunca se guarda, la sal de un día pasado se
borra, y nadie puede ser seguido de un día a otro. Los bots se descartan. El
país viene del proxy delante de Rustrak (Cloudflare, Vercel, CloudFront) o,
si no, del idioma del visitante.

Cuánto tiempo se guardan las páginas vistas y los eventos se decide en la
página de **Almacenamiento** de Rustrak.

## Permisos

**Tráfico web: leer** permite ver el informe de un proyecto. Dalo en un rol,
en Roles, o en los permisos propios de un miembro.

**Visitantes: leer** se da aparte, porque muestra a las personas una a una: la
página de Visitantes, el día y el perfil de cada visitante, y las últimas
veces que ocurrió un evento con quién lo hizo. Dalo también a quien deba ver
eso.
