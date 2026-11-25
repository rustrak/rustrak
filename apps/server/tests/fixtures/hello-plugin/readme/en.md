# Hello

The smallest Rustrak plugin. It counts hits per project and is the fixture
the server's plugin tests run against.

## What it adds

- **Hits**, a page in each project: every page view, and a form to add one.
- **About Hello**, and a page of its own outside any project.
- A widget under every issue with the project's hit count.

## Sending a hit

```bash
curl -X POST "https://rustrak.example.com/api/projects/1/plugins/hello/hit?sentry_key=<key>" \
  --data '/pricing'
```

## Settings

| Setting | Where | Default |
|---|---|---|
| Greeting | Instance | `Hello` |
| Signing token | Instance | not set |
| Longest path | Each project | `200` |

> Hits are history: how long they are kept is set in Rustrak's Storage page.
