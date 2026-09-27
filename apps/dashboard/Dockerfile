# The standalone dashboard image: the SPA compiled by Vite, served by nginx,
# with the API proxied to the server named by RUSTRAK_API_URL at start.
#
# This is not the default deployment. The server image already carries the
# dashboard and serves it from the same process, and that is the one to reach
# for. This image is for the other topology: the API on a small box and the
# dashboard somewhere else, with the machine holding the data serving no
# frontend at all. It works the same way `vite dev` does, so the browser still
# sees one origin and the session cookie stays first-party.
#
# Build context is the repository root, because the dashboard depends on
# `@rustrak/client` from the workspace.

# ── Stage 1: prune the workspace to what the dashboard needs ────────────────
FROM node:24-alpine AS base
ENV PNPM_HOME="/pnpm"
ENV PATH="$PNPM_HOME:$PATH"
RUN corepack enable

FROM base AS pruner
WORKDIR /app
COPY . .
RUN pnpm dlx turbo prune @rustrak/dashboard --docker

# ── Stage 2: install and build ───────────────────────────────────────────────
FROM base AS builder
WORKDIR /app
COPY --from=pruner /app/out/json/ .
RUN --mount=type=cache,id=pnpm,target=/pnpm/store pnpm install --frozen-lockfile
COPY --from=pruner /app/out/full/ .
# `@rustrak/client` is a workspace dependency compiled by its own build.
RUN pnpm --filter @rustrak/client build && pnpm --filter @rustrak/dashboard build

# ── Stage 3: nginx ───────────────────────────────────────────────────────────
# `nginx-unprivileged` runs as uid 101 from the first instruction and listens
# on a port above 1024, so there is no root at any point of the container's
# life and nothing to drop.
FROM nginxinc/nginx-unprivileged:1.29-alpine-slim

# The docs address this image on 3000. `PORT` is read by the template at
# start, so it can still be changed.
ENV PORT=3000
ENV RUSTRAK_API_URL=http://server:8080
# Makes the entrypoint export the container's DNS as NGINX_LOCAL_RESOLVERS,
# which the template needs to resolve RUSTRAK_API_URL's host per request.
ENV NGINX_ENTRYPOINT_LOCAL_RESOLVERS=1
EXPOSE 3000

# The entrypoint runs envsubst over /etc/nginx/templates/*.template into
# /etc/nginx/conf.d/ on every start. That is what turns RUSTRAK_API_URL into
# configuration without a rebuild.
RUN rm /etc/nginx/conf.d/default.conf
COPY apps/dashboard/docker/nginx.conf.template /etc/nginx/templates/rustrak.conf.template
# Sourced before the template is rendered (name order, `.envsh` means sourced):
# drops a trailing slash from RUSTRAK_API_URL and refuses a path, because with
# a variable `proxy_pass` either would send every API request to the wrong URI.
COPY apps/dashboard/docker/15-rustrak-api-url.envsh /docker-entrypoint.d/15-rustrak-api-url.envsh

COPY --from=builder /app/apps/dashboard/dist /usr/share/nginx/html
