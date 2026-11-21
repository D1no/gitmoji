# The built page in a small web server that runs as an ordinary user.
# Build the page first (`pnpm build`); this image only carries dist/.
FROM nginxinc/nginx-unprivileged:1.29-alpine-slim
COPY docker/nginx.conf /etc/nginx/conf.d/default.conf
COPY dist/ /usr/share/nginx/html/
