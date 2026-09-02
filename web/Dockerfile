# syntax=docker/dockerfile:1.6
FROM node:20-alpine AS builder
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
RUN npm run build

FROM node:20-alpine AS runner
WORKDIR /app
ENV NODE_ENV=production
ENV PORT=3005
ENV HOSTNAME=0.0.0.0
RUN addgroup --system --gid 1001 nodejs && adduser --system --uid 1001 nextjs
COPY --from=builder /app/package*.json ./
COPY --from=builder /app/.next ./.next
COPY --from=builder /app/public ./public
COPY --from=builder /app/node_modules ./node_modules
COPY --from=builder /app/next.config.js ./next.config.js
USER nextjs
EXPOSE 3005
HEALTHCHECK --interval=15s --timeout=5s --retries=5 CMD wget --no-verbose --tries=1 --spider http://localhost:3005/ || exit 1
CMD ["npm", "start"]
