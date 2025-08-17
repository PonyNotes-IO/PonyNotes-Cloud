# 指定使用 Docker Buildx 的 Dockerfile 语法版本 1，支持多阶段构建等高级功能
# syntax=docker/dockerfile:1

# 第一阶段：构建阶段
# 使用 golang 官方镜像作为基础镜像，并命名为 "base" 阶段，用于编译 Go 应用
FROM golang as base

# 设置工作目录为 /go/src/supabase/auth，这是 Go 项目的标准路径结构
WORKDIR /go/src/supabase/auth

# 将本地的 gotrue/ 目录（包含修改过的 GoTrue 源代码，集成了阿里云短信功能）复制到容器的当前工作目录
COPY gotrue/ ./

# 编译 Go 应用程序
# CGO_ENABLED=0: 禁用 CGO，生成静态链接的二进制文件，不依赖 C 库，便于在 Alpine 等精简镜像中运行
# go build: 编译 Go 程序
# -o /auth: 指定输出的可执行文件路径为 /auth
# ./cmd: 指定要编译的包路径，通常 cmd 目录包含 main 函数
RUN CGO_ENABLED=0 go build -o /auth ./cmd

# 第二阶段：运行阶段
# 使用 Alpine Linux 3.20 作为最终运行镜像，Alpine 是一个安全、轻量级的 Linux 发行版
FROM alpine:3.20

# 创建一个非 root 用户用于运行应用，提高安全性
# -D: 不创建密码（禁用密码登录）
# -u 1000: 指定用户 ID 为 1000
# supabase: 用户名
RUN adduser -D -u 1000 supabase

# 安装必要的系统包
# --no-cache: 不缓存包索引，减小镜像大小
# ca-certificates: SSL/TLS 证书包，用于 HTTPS 连接验证
# curl: HTTP 客户端工具，常用于健康检查和调试
RUN apk add --no-cache ca-certificates curl

# 切换到 supabase 用户，避免以 root 用户运行应用，增强安全性
USER supabase

# 从构建阶段复制编译好的二进制文件到当前目录
# --from=base: 指定从名为 "base" 的构建阶段复制
COPY --from=base /auth .

# 从构建阶段复制数据库迁移文件到 migrations 目录
# 这些文件用于数据库结构的版本控制和自动迁移
COPY --from=base /go/src/supabase/auth/migrations ./migrations

# 复制启动脚本到容器中
# 该脚本负责配置环境变量、执行数据库迁移等初始化操作
COPY PonyNotes-Cloud/docker/gotrue/start.sh .

# 设置容器启动时的默认命令
# 执行 start.sh 脚本来启动 GoTrue 认证服务
CMD ["./start.sh"]
