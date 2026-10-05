# BeoDesk

Ứng dụng remote desktop cho Windows, Ubuntu và Android, đang xây dựng nền tảng M0 và prototype LAN đầu tiên. Baseline phát triển là **Ubuntu 22.04 x64 / GNOME X11**.

Hiện có Flutter gọi Rust thật, định danh trong kho khóa hệ điều hành và **xem màn hình liên tục, điều khiển chuột/bàn phím qua QUIC/TLS 1.3**, máy xem xác thực vân tay máy chia sẻ. Máy chia sẻ phải cho phép từng phiên. Host hiện hỗ trợ Ubuntu GNOME X11. Luồng trực tiếp dùng H.264 đóng kèm ứng dụng, tối đa 1080p/30 fps và texture native trên Linux; chưa nghiệm thu hiệu năng M1 trên hai máy. Chế độ nhận một ảnh vẫn có nút riêng.

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

## Xem và điều khiển qua LAN

1. Chạy BeoDesk trên hai máy Ubuntu GNOME X11 trong cùng LAN; lấy IP và dấu vân tay đầy đủ của máy chia sẻ qua kênh tin cậy.
2. Máy 2 (được điều khiển): bấm **Chia sẻ màn hình máy này**, ứng dụng tự lấy IP. Bấm **Bật chia sẻ**, rồi **Sao chép IP + vân tay** để gửi thông tin cho máy 1. Không cần nhập vân tay máy 1.
3. Máy 1 (điều khiển): nhập IP và vân tay máy 2, hoặc dùng **Dán IP + vân tay**, rồi chọn **Kết nối và điều khiển**. IP không có cổng sẽ dùng `4433`; vẫn nhập được `IP:cổng` khác.
4. Máy chia sẻ xác nhận **Cho phép điều khiển**. Máy xem mở cửa sổ **Màn hình trực tiếp**; bấm vào ảnh để dùng bàn phím. Hỗ trợ di chuyển, click trái/giữa/phải, kéo chuột và cuộn dọc.
5. Bấm **Điều khiển** để chuyển sang chỉ xem, hoặc **Ctrl + Alt + Esc** để nhả focus bàn phím. Kết thúc bằng **Ngắt kết nối** ở máy xem hoặc **Dừng chia sẻ** ở máy chia sẻ. Các phím/nút đang giữ được nhả khi mất focus, ngắt phiên hoặc mất lease 2 giây.

Cập nhật bản mới trên **cả hai máy** trước khi thử phiên trực tiếp. Nút **Nhận ảnh màn hình** vẫn chỉ chụp một ảnh sau xác nhận **Cho phép một ảnh**.

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

Kiểm tra capture → H.264/PNG → QUIC → hiển thị trên localhost và màn hình ảo (cần phiên GNOME đang mở khóa):

```bash
bash scripts/dev.sh snapshot-smoke
bash scripts/dev.sh lan-smoke
bash scripts/dev.sh live-smoke
bash scripts/dev.sh live-gui-smoke
```

`snapshot-smoke` tự cho phép yêu cầu loopback của chính bài kiểm tra, chỉ chụp màn hình Xvfb và ghi `.local/lan-snapshot.png`. `lan-smoke` kiểm tra cả giao diện xác nhận, bridge Flutter–Rust và hiển thị ảnh trên Xvfb; không lưu ảnh. Ứng dụng thông thường luôn yêu cầu xác nhận tại host.

`live-smoke` dùng cửa sổ riêng trên Xvfb để xác nhận khung hình thay đổi, click/drag/cuộn, Shift+A, nhả input khi mất lease và khi host dừng chia sẻ, rồi kết nối lại. `live-gui-smoke` kiểm tra nút kết nối, hộp thoại cấp quyền điều khiển, nhiều khung hình qua bridge thật và ngắt phiên từ Flutter. Các bài này cần GNOME Keyring đang mở khóa; không thao tác trên desktop thật.

## Cấu trúc

| Đường dẫn | Vai trò |
|---|---|
| `crates/remote-protocol` | Schema Protobuf, giới hạn gói tin, kiểm tra version/payload |
| `crates/remote-core` | Khóa thiết bị, quyền phiên, timeout, sequence và input cleanup |
| `crates/remote-network` | QUIC/TLS xác thực hai chiều, phiên liên tục, input lease và chế độ một ảnh |
| `crates/remote-capture` | Capture X11, kiểm tra khóa GNOME và PNG có giới hạn |
| `crates/remote-video` | H.264, giảm độ phân giải, điều chỉnh bitrate và giới hạn bộ giải mã |
| `crates/remote-input` | Chuột/phím XTEST, ánh xạ phím vật lý XKB và nhả input theo phiên |
| `crates/remote-bridge` | API Flutter–Rust và kho khóa hệ điều hành |
| `apps/flutter` | Giao diện Linux/Windows/Android, widget và integration tests |
| `scripts` | Môi trường SDK cục bộ và lệnh phát triển |
| `.github/workflows/ci.yml` | Kiểm tra desktop và Android compile trên CI |

Chạy `bash scripts/dev.sh generate` sau khi thay API Rust trong `crates/remote-bridge/src/api`; không sửa bindings sinh tự động bằng tay. Protobuf được sinh lúc Cargo build bằng `protoc` vendored, không cần cài `protoc` hệ thống.

## Tài liệu

- [Luồng video H.264 và số đo](docs/video-pipeline.md)
- [Môi trường và phiên bản](docs/development.md)
- [Quyết định nền tảng và giới hạn hiện tại](docs/decisions/0001-foundation.md)
- [Kết nối bằng vân tay máy chia sẻ và xác nhận tại host](docs/decisions/0003-attended-connect.md)
- [Kế hoạch sản phẩm](remote-desktop-development-plan.md)

Linux và Android ARM64 được build tại máy phát triển; Windows và kiểm thử hai máy thật còn cần nghiệm thu. CI chỉ chạy khi repository được đưa lên dịch vụ Git có GitHub Actions; tạo workflow không đồng nghĩa CI đã chạy.
