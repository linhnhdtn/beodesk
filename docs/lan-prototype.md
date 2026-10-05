# LAN viewing and control prototype

Prototype Ubuntu GNOME X11 hỗ trợ màn hình cập nhật liên tục và điều khiển chuột/bàn phím sau khi host cho phép. Luồng trực tiếp hiện dùng H.264 và texture native Linux, giới hạn 1080p/30 fps; chưa nghiệm thu ngưỡng hiệu năng M1 trên hai máy. Chế độ chụp một ảnh vẫn được giữ.

## Chạy trên hai máy

Trên mỗi máy, chạy bản mới trong phiên GNOME X11 và mở khóa GNOME Keyring nếu hệ điều hành yêu cầu.

1. **Máy 2 — được điều khiển:** bấm **Chia sẻ màn hình máy này**. Ứng dụng tự lấy IP; chọn IP mạng chung khi có nhiều địa chỉ. Bấm **Bật chia sẻ**. Không có ô nhập vân tay máy 1.
2. Bấm **Sao chép IP + vân tay** trong popup hoặc ngay khi chia sẻ đang bật để gửi thông tin của máy 2 cho máy 1 qua kênh tin cậy.
3. **Máy 1 — điều khiển:** nhập IP + vân tay máy 2 hoặc bấm **Dán IP + vân tay**, rồi **Kết nối và điều khiển**. Chỉ nhập IP sẽ dùng cổng `4433`; có thể nhập `IP:cổng` khác.
4. **Máy 2:** bấm **Cho phép điều khiển** trong hộp thoại yêu cầu. Hộp thoại tự hiển thị vân tay máy 1 từ kết nối TLS. Chưa cho phép thì chưa chụp hình hoặc nhận input; có thể từ chối hoặc để yêu cầu hết hạn sau 60 giây.

Dấu vân tay máy 2 vẫn cần đủ 64 ký tự hex và được máy 1 kiểm tra. Dấu vân tay là thông tin công khai, không phải mật khẩu. Khóa riêng không được trao đổi. Máy 1 tự chứng minh sở hữu khóa thiết bị qua TLS; máy 2 quyết định cấp quyền từng phiên tại hộp thoại. Không tự cho phép chỉ vì người khác biết IP/vân tay.

Thông tin sao chép có dạng:

```text
BeoDesk
IP: 192.168.1.20:4433
Dấu vân tay: <đầy đủ 64 ký tự hex, chia thành 8 nhóm>
```

Nút **Dán IP + vân tay** cũng nhận vân tay thuần và giữ nguyên IP đang nhập. Clipboard thiếu hoặc sai định dạng không được áp dụng một phần. Tự lấy IP không bật listener; listener chỉ bật khi bấm **Bật chia sẻ**. Có thể chỉnh địa chỉ thủ công.

Trong **Màn hình trực tiếp**, bấm vào ảnh để lấy focus bàn phím. Chuột hỗ trợ trái/giữa/phải, kéo thả và cuộn dọc. Tọa độ tính theo vùng ảnh thực tế, bỏ qua viền trống khi tỷ lệ cửa sổ khác màn hình host. Phím dùng HID → tên phím vật lý XKB; layout ở host quyết định ký tự. Bấm **Điều khiển** để chuyển sang chỉ xem; **Ctrl + Alt + Esc** nhả focus và input đang giữ. **Ngắt kết nối** hoặc đóng cửa sổ kết thúc phiên; host có chỉ báo phiên đang hoạt động và nút **Dừng chia sẻ**.

Nút **Nhận ảnh màn hình** vẫn yêu cầu **Cho phép một ảnh** và chỉ chụp một lần. Sao chép/dán thông tin không tự bật listener hay tự chấp thuận yêu cầu.

Giữ máy chia sẻ mở khóa. Ứng dụng kiểm tra `org.gnome.ScreenSaver.GetActive` khi bật listener, trước/sau capture, trước input và định kỳ trong phiên. Khi phát hiện khóa hoặc không xác minh được trạng thái, phiên kết thúc và nhả input. **Dừng chia sẻ** đóng listener và kết nối hiện tại. Viewer gửi lease mỗi 500 ms khi giao diện còn polling; sau 2 giây không có lease, host kết thúc phiên và nhả phím/nút. Input và frame không chặn nhau khi mạng chậm; mỗi phía giữ bộ đệm có giới hạn. Listener không tự bật lại khi mở ứng dụng; quyền điều khiển không được lưu để tự cấp lại.

UDP tại cổng đã nhập phải đi được giữa hai máy. Ứng dụng không tự sửa firewall, NAT hoặc router và chưa có relay/TCP fallback. Địa chỉ là IP, tùy chọn port; IPv6 có cổng dùng dạng `[fd00::20]:4433`. Không nhập hostname, Device ID hay địa chỉ bind wildcard. IPv6 chưa được nghiệm thu qua hai máy thật.

## Khi báo lỗi kết nối

Lỗi `Local user stopped sharing` ngay sau **Cho phép điều khiển** từng có thể do phản hồi kiểm tra trạng thái về muộn làm hộp thoại đóng thêm lần nữa và đóng nhầm trang host. Bản sửa hủy timer trước khi đóng hộp thoại, bỏ qua phản hồi đến muộn và chỉ đóng đúng route hiện tại. Đồng thời, thay đổi kích thước cửa sổ không còn hủy `LanPanel` hoặc dừng phiên. Cần thay bản chạy cũ ở máy chia sẻ để nhận bản sửa này.

Nếu báo `connection lost`, xem **Chi tiết** để biết lỗi xác thực hay máy chia sẻ đã kết thúc phiên. Không dùng `127.0.0.1` để kết nối sang máy khác.

Nếu Chi tiết chứa `aborted by peer` và `Peer device fingerprint does not match the verified pin`, máy chia sẻ có thể còn dùng bản cũ yêu cầu nhập vân tay máy xem. Cập nhật bản mới trên máy chia sẻ và bật lại chia sẻ. Bản mới không yêu cầu trao đổi vân tay ngược từ máy 1 về máy 2.

Nếu lỗi pin nằm ở bước `Authenticating LAN host at ...` và không có `aborted by peer`, máy xem đang từ chối dấu vân tay host. Sao chép lại **IP + vân tay** từ máy chia sẻ, đối chiếu đầy đủ qua kênh tin cậy rồi dán vào form nhận ảnh. Ứng dụng không tự thay dấu vân tay đã tin cậy hoặc bỏ kiểm tra xác thực khi lỗi.

Bản mới có nút **Chi tiết** ở thông báo lỗi, giữ cả nguyên nhân Rust/TLS bên dưới dòng đầu. Khi chạy `./beodesk` từ terminal trên host, lỗi xác thực cũng xuất hiện dưới tiền tố `BeoDesk LAN peer authentication`. Các log này chỉ chứa thông tin lỗi, không có khóa riêng. Lỗi pin không khớp được tái hiện bằng kiểm thử QUIC thật; ảnh chụp chỉ có dòng `connection lost` chưa đủ để xác định chắc chắn nguyên nhân của một kết nối cụ thể.

Nếu báo timeout hoặc connection refused, kiểm tra host đang hiện **Đang chờ yêu cầu tại ...**, địa chỉ/cổng đúng, hai máy đi được UDP và firewall không chặn cổng. Nếu xác thực thành công nhưng host báo không thể capture, kiểm tra GNOME X11, desktop mở khóa và giới hạn ảnh ở phần dưới.

## Giới hạn

- Host chỉ hỗ trợ Ubuntu GNOME X11. Windows capture và Wayland portal chưa có; Android chưa có secure storage nên chưa dùng làm controller được.
- Capture toàn bộ root desktop X11, chưa chọn monitor. Tổng số pixel tối đa `3840 × 2160`; desktop ghép nhiều màn hình vượt giới hạn sẽ bị từ chối.
- H.264 baseline, tối đa 1920×1080 và 30 fps, qua reliable QUIC stream. Mỗi khung mã hóa tối đa 2 MiB, chỉ một khung đang chờ phía nhận giải mã; mạng chậm giảm nhịp capture và bitrate. Texture Linux giữ khung mới nhất. Chế độ một ảnh dùng PNG tối đa 8 MiB. Xem [luồng video và số đo](video-pipeline.md).
- Chưa có texture Windows/Android, mã hóa phần cứng, đồng bộ clipboard giữa hai máy, file transfer, unattended access hoặc device discovery. Clipboard cục bộ chỉ dùng cho nút sao chép/dán thông tin kết nối.
- Bàn phím vật lý đã thử với layout US và các modifier. IME, nhập văn bản Android và tổ hợp phím bị hệ điều hành máy xem giữ lại chưa được nghiệm thu. Chuyển sang chỉ xem là tạm dừng gửi input ở viewer trong phiên đã được host cấp quyền.
- Mỗi host xử lý tuần tự một yêu cầu; một viewer chỉ có một yêu cầu đang chạy. Tắt ứng dụng sẽ kết thúc listener, không có dịch vụ nền.
- Giao diện chỉ giữ ảnh nhận trong bộ nhớ. Riêng bài kiểm tra `snapshot-smoke` ghi ảnh của màn hình ảo vào `.local/lan-snapshot.png`.

## Kiểm tra đã thực hiện

Bản kết nối đơn giản đạt 40 Rust tests, 36 Flutter tests, hai bài GUI native (live/snapshot), native XTEST smoke và Linux release build. Các bài kiểm tra bao gồm máy chia sẻ không cần nhập vân tay máy xem, máy xem chỉ nhập IP + vân tay host, mặc định cổng 4433, giữ kiểm tra vân tay host, chứng chỉ client bắt buộc và không truy cập desktop trước khi host cho phép.

Ngày 2026-10-05: chuyển phiên trực tiếp sang H.264 + texture native Linux. **38 Rust tests, 34 Flutter tests**, Clippy và Flutter analyze đạt; native GUI integration và XTEST smoke đạt. Probe release trên localhost/Xvfb đo **28,9 FPS giải mã ở 1920×1080 trong 3,01 giây**, với cảnh đơn giản, chủ yếu tĩnh. Linux release đã đóng gói; chưa đo trên hai máy. Xem [chi tiết và giới hạn số đo](video-pipeline.md).

Tại Ubuntu 22.04.5 x64 / GNOME X11, ngày 2026-10-01–02:

Phiên trực tiếp bổ sung ngày 2026-10-02: tổng 33 Rust tests và 28 Flutter tests đạt. Có 6 kiểm thử QUIC mới cho consent, nhiều khung hình, quyền xem/điều khiển, nhả input, mất lease, dừng host và khóa desktop; 6 widget tests cho cập nhật ảnh, ánh xạ tọa độ, phím modifier, mất focus, consent và thông báo lỗi dài. `live-smoke` đã kiểm tra PNG thay đổi ở 1280×720, XTEST click/drag/cuộn/Shift+A thật, trạng thái phím/nút sau mất lease, nối lại và dừng host. `live-gui-smoke` đã kiểm tra cấp quyền điều khiển qua Flutter, nhiều khung hình qua bridge thật và ngắt phiên. Linux release build thành công. Những kiểm thử này dùng localhost/Xvfb; chưa thay thế nghiệm thu hai máy.

Bản sửa vòng đời hộp thoại bổ sung 4 kiểm thử, nâng tổng Flutter lên 32 bài đạt: phản hồi trạng thái đến muộn sau khi cấp quyền, giữ host/consent khi đổi bố cục, giữ yêu cầu đang kết nối và giữ phiên đang hoạt động khi resize. Các tình huống này đã thất bại với mã cũ trước khi sửa.

`live-gui-smoke` cũng đã chạy với phản hồi trạng thái bị trì hoãn đến sau khi cấp quyền qua bridge thật; host vẫn hoạt động, nhận nhiều khung hình và chỉ ngắt khi bấm nút kết thúc. Bản Linux release đã build lại.

Kết quả baseline snapshot trước khi thêm phiên trực tiếp:

- 26 Rust tests đạt, gồm 9 bài UDP QUIC thực tế; sai pin ở cả hai phía bị từ chối, gói khai báo quá lớn bị chặn trước khi đọc body, từ chối/ngắt khi đang chờ không gọi capture. Lỗi do host nhập sai pin viewer giữ được nguyên nhân xác thực; kiểm thử phân biệt lỗi host từ chối viewer (`aborted by peer`) với viewer từ chối host (`Authenticating LAN host at ...`).
- 22 Flutter tests đạt: 19 widget tests cho giao diện, quyền phiên, sao chép/dán đúng vai trò, hướng dẫn đúng phía lỗi pin và hiển thị pin máy xem sau khi bật lại chia sẻ; 3 tests kiểm tra định dạng thông tin kết nối, IPv6 và từ chối dữ liệu không hợp lệ. Clipboard trong widget tests được giả lập, không đọc/sửa clipboard thật của người dùng.
- Bài loopback dùng định danh GNOME Keyring thật, màn hình Xvfb 1280×720 và xác nhận do chính test thực hiện đã nhận PNG hợp lệ.
- Native Flutter integration trên Xvfb đã lấy IP thật của chính máy bằng nút mới, bật host ở địa chỉ đó, gửi yêu cầu qua QUIC từ cùng máy, xác nhận bằng hộp thoại và hiển thị PNG qua bridge thật. Kiểm tra trạng thái tải kết thúc khi ảnh xuất hiện đã đạt. Bài này vẫn chỉ chạy trên một máy.

Chạy `bash scripts/dev.sh lan-smoke` để lặp lại bài giao diện đầu cuối, hoặc `bash scripts/dev.sh snapshot-smoke` để kiểm tra luồng Rust/capture. Cả hai cần GNOME Keyring và desktop thật đang mở khóa, nhưng chỉ chụp màn hình ảo của bài kiểm tra.

Đây là kiểm tra trên một máy và màn hình ảo. Chưa có số đo độ trễ/fps hoặc nghiệm thu LAN hai máy, Windows, Android runtime. Hướng dẫn build nằm ở [development.md](development.md).
