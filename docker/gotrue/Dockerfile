# syntax=docker/dockerfile:1
FROM golang as base
WORKDIR /go/src/supabase/auth
# Copy our modified GoTrue source code with Aliyun SMS integration
COPY gotrue/ ./
RUN CGO_ENABLED=0 go build -o /auth ./cmd

FROM alpine:3.20
RUN adduser -D -u 1000 supabase

RUN apk add --no-cache ca-certificates curl
USER supabase

COPY --from=base /auth .
COPY --from=base /go/src/supabase/auth/migrations ./migrations

COPY PonyNotes-Cloud/docker/gotrue/start.sh .
CMD ["./start.sh"]
