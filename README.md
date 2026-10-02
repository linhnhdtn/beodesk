# BeoDesk

Ứng dụng remote desktop cho Windows, Ubuntu và Android, đang xây dựng nền tảng M0 và prototype LAN đầu tiên. Baseline phát triển là **Ubuntu 22.04 x64 / GNOME X11**.

Hiện có Flutter gọi Rust thật, định danh trong kho khóa hệ điều hành và **nhận một ảnh màn hình qua QUIC/TLS 1.3**, xác thực dấu vân tay đầy đủ ở cả hai phía. Máy chia sẻ phải cho phép từng ảnh; từ chối, ngắt kết nối hoặc hết hạn không được gọi capture. Host hiện hỗ trợ Ubuntu GNOME X11. Chưa có video liên tục, H.264 hay điều khiển chuột/phím; M1 chưa hoàn thành.

## Chạy trên máy phát triển

Toolchain đã cài riêng trong `.tools/`, không cần sửa PATH toàn hệ thống:

```bash
bash scripts/dev.sh doctor
bash scripts/dev.sh run
```

Hoặc mở terminal mới và chạy:

```bash
source scripts/env.sh
cd apps/flutter
flutter run -d linux
```

Linux dùng Secret Service của phiên đăng nhập (thường là GNOME Keyring); Windows dùng Credential Manager. Khi kho khóa bị khóa, mở khóa bằng hộp thoại của hệ điều hành. Ứng dụng không lưu khóa riêng vào file cấu hình và không tự thay khóa khi đọc thất bại. Android mới có target build; lưu khóa bằng Android Keystore chưa được triển khai.

## Thử xem ảnh qua LAN

1. Chạy BeoDesk trên hai máy Ubuntu GNOME X11 trong cùng LAN; trao đổi và đối chiếu dấu vân tay đầy đủ qua kênh tin cậy.
2. Máy chia sẻ: bấm **Lấy IP + cổng** ngay cạnh **Chia sẻ màn hình máy này**. Trong popup, dùng **Sao chép IP + vân tay** để lấy thông tin của máy này gửi cho máy xem; dùng **Dán vân tay máy xem** để điền dấu vân tay nhận từ người kia rồi bật chia sẻ. Nếu có nhiều địa chỉ, chọn IP của mạng chung; cổng mặc định là `4433`, vẫn có thể sửa thủ công.
3. Máy xem: sao chép nội dung được máy chia sẻ gửi, bấm **Dán IP + vân tay** trên màn hình chính để điền cả hai ô, rồi chọn **Nhận ảnh màn hình**.
4. Máy chia sẻ xác nhận **Cho phép một ảnh**; máy xem hiển thị ảnh có thể phóng to. Dừng listener bằng **Dừng chia sẻ**.

Ứng dụng không tự mở firewall hoặc router; cần UDP giữa hai máy tại cổng đã chọn. Chỉ bind IP cụ thể, không dùng `0.0.0.0`. Màn hình đang khóa hoặc không xác minh được trạng thái khóa GNOME sẽ bị từ chối. Xem [hướng dẫn và giới hạn prototype](docs/lan-prototype.md).

## Kiểm tra và build

```bash
bash scripts/dev.sh test
bash scripts/dev.sh smoke
bash scripts/dev.sh build
```

Bản Linux release nằm tại `apps/flutter/build/linux/x64/release/bundle/`. Phải giữ cả thư mục bundle, gồm executable, `lib/` và `data/`.

Kiểm tra kho khóa thật (tạo định danh nếu chưa có):

```bash
source scripts/env.sh
cargo run -p remote_bridge --example identity_probe
```

Kiểm tra capture → QUIC → PNG trên localhost và màn hình ảo (cần phiên GNOME đang mở khóa):

```bash
bash scripts/dev.sh snapshot-smoke
bash scripts/dev.sh lan-smoke
```

`snapshot-smoke` tự cho phép yêu cầu loopback của chính bài kiểm tra, chỉ chụp màn hình Xvfb và ghi `.local/lan-snapshot.png`. `lan-smoke` kiểm tra cả giao diện xác nhận, bridge Flutter–Rust và hiển thị ảnh trên Xvfb; không lưu ảnh. Ứng dụng thông thường luôn yêu cầu xác nhận tại host.

## Cấu trúc

| Đường dẫn | Vai trò |
|---|---|
| `crates/remote-protocol` | Schema Protobuf, giới hạn gói tin, kiểm tra version/payload |
| `crates/remote-core` | Khóa thiết bị, quyền phiên, timeout, sequence và input cleanup |
| `crates/remote-network` | QUIC/TLS xác thực hai chiều, framing và luồng xác nhận một ảnh |
| `crates/remote-capture` | Capture X11, kiểm tra khóa GNOME và PNG có giới hạn |
| `crates/remote-bridge` | API Flutter–Rust và kho khóa hệ điều hành |
| `apps/flutter` | Giao diện Linux/Windows/Android, widget và integration tests |
| `scripts` | Môi trường SDK cục bộ và lệnh phát triển |
| `.github/workflows/ci.yml` | Kiểm tra desktop và Android compile trên CI |

Chạy `bash scripts/dev.sh generate` sau khi thay API Rust trong `crates/remote-bridge/src/api`; không sửa bindings sinh tự động bằng tay. Protobuf được sinh lúc Cargo build bằng `protoc` vendored, không cần cài `protoc` hệ thống.

## Tài liệu

- [Môi trường và phiên bản](docs/development.md)
- [Quyết định nền tảng và giới hạn hiện tại](docs/decisions/0001-foundation.md)
- [Transport và prototype ảnh qua LAN](docs/decisions/0002-lan-snapshot.md)
- [Kế hoạch sản phẩm](remote-desktop-development-plan.md)

Linux và Android ARM64 được build tại máy phát triển; Windows và kiểm thử hai máy thật còn cần nghiệm thu. CI chỉ chạy khi repository được đưa lên dịch vụ Git có GitHub Actions; tạo workflow không đồng nghĩa CI đã chạy.
