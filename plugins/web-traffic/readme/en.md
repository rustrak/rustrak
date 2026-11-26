# Web Traffic

Privacy-first web analytics inside Rustrak: who visits your site, where they
come from and what they read, next to the errors they hit. No cookies, no
consent banner, no third party.

## What you get

Filters on every page (page, referrer, campaign, country, browser…), set by
clicking any value, and saved views like the rest of Rustrak.

- **Visitors, page views and bounce rate**, against the period before.
- **Online now**: visitors in the last five minutes.
- **Top pages, referrers, sources and campaigns** (`utm_*` and `ref=`).
- **Countries, browsers, operating systems and devices.**
- **Your own events**: a signup, a purchase.
- **Traffic on an issue's page**: on every issue, how many people saw the page
  it happens on, so you know what the error weighs.
- **Last 24 hours** at the end of each project's overview.
- **Entry and exit pages**, and any list in full with its bounce rate and
  visit length.
- **Visitors**: each one's day, visit by visit, with where they came from.
- **Events**: over time, their properties, the pages they happen on.

## Set up

1. In **Settings › Plugins › Web Traffic › Configuration**, turn it on for a
   project.
2. Open that project there: its **Setup** has the script with the project's
   key. Paste it into the `<head>` of every page:

```html
<script defer src="https://rustrak.example.com/public/web-traffic/script.js"
  data-project="1" data-key="<the project's DSN public key>"></script>
```

Single-page apps are followed through the History API. To count an event:

```js
webTraffic.track('signup', { plan: 'pro' });
```

```html
<button data-traffic-event="buy">Buy</button>
```

## Settings

| Setting | Where | What |
| --- | --- | --- |
| Domains | project | Count only these sites (subdomains included). Empty counts any. |
| Keep query strings | project | Store `?search=…` with each page. |
| Respect Do Not Track | instance | Skip visitors whose browser asks not to be tracked. On by default. |

## Privacy

A visitor is a hash of their address, their browser and a salt that is
replaced every day. The address is never stored, the salt of a past day is
deleted, and nobody can be followed from one day to the next. Bots are
dropped. The country comes from the proxy in front of Rustrak (Cloudflare,
Vercel, CloudFront) or else from the visitor's language.

How long page views and events are kept is set in Rustrak's **Storage** page.

## Permissions

**Web traffic: read** lets someone see a project's report. Give it in a role
under Roles or a member's own permissions.

**Visitors: read** is given apart from it, because it shows people one by
one: the Visitors page, each visitor's day and profile, and an event's latest
occurrences with who did them. Give it as well to whoever should see those.
