# syntax=docker/dockerfile:1.6
FROM node:20.19.5-alpine3.22 AS builder
WORKDIR /app
COPY package*.json ./
RUN npm ci --legacy-peer-deps
COPY . .
ARG NEXT_PUBLIC_MAIL_API=https://mail.aivory.uk
ARG NEXT_PUBLIC_BOOK_URL=https://book.aivory.uk/book/aivory-call
ARG NEXT_PUBLIC_MAIL_MX_HOST=mail.aivory.uk
ENV NEXT_PUBLIC_MAIL_API=$NEXT_PUBLIC_MAIL_API
ENV NEXT_PUBLIC_BOOK_URL=$NEXT_PUBLIC_BOOK_URL
ENV NEXT_PUBLIC_MAIL_MX_HOST=$NEXT_PUBLIC_MAIL_MX_HOST
ENV NEXT_TELEMETRY_DISABLED=1
RUN npm run build && npm prune --omit=dev --legacy-peer-deps

FROM node:20.19.5-alpine3.22 AS runner
WORKDIR /app
ENV NODE_ENV=production
ENV PORT=3005
ENV HOSTNAME=0.0.0.0
ENV HOME=/tmp
ENV TMPDIR=/tmp
ENV NEXT_TELEMETRY_DISABLED=1
RUN addgroup --system --gid 1001 nodejs && adduser --system --uid 1001 --ingroup nodejs nextjs
COPY --from=builder --chown=1001:1001 /app/.next ./.next
COPY --from=builder --chown=1001:1001 /app/public ./public
COPY --from=builder --chown=1001:1001 /app/node_modules ./node_modules
COPY --from=builder --chown=1001:1001 /app/next.config.js ./next.config.js
USER 1001:1001
EXPOSE 3005
HEALTHCHECK --interval=15s --timeout=5s --start-period=10s --retries=5 CMD node -e "fetch('http://127.0.0.1:3005/').then(r => process.exit(r.ok ? 0 : 1)).catch(() => process.exit(1))"
CMD ["./node_modules/.bin/next", "start", "-p", "3005"]
