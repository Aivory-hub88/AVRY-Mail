# Aivory Mail — dovecot image with service users + dirs baked in.
# The stock image ships no dovenull/dovecot users and no mkdir binary,
# so runtime user creation is impossible; bake everything at build time.
# Mail access uses numeric mail_uid/mail_gid = 1000 (matches avry-mail's
# writer uid), so only dovecot/dovenull service users are created here.
FROM dovecot/dovecot:latest
USER root
RUN groupadd -r dovecot || true
RUN useradd -r -g dovecot -d /nonexistent -s /usr/sbin/nologin dovecot || true
RUN useradd -r -g dovecot -d /nonexistent -s /usr/sbin/nologin dovenull || true
COPY --chown=1000:1000 dovecot/empty-vmail/.keep /var/vmail/.keep
COPY dovecot/empty-run/.keep /run/dovecot/.keep
