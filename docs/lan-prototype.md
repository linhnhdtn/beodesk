# LAN snapshot prototype

Prototype Ubuntu GNOME X11 này kiểm chứng luồng định danh → kết nối mã hóa → người dùng cho phép → chụp màn hình → hiển thị ảnh. Đây chưa phải phiên remote desktop liên tục.

## Chạy trên hai máy

Trên mỗi máy, chạy `bash scripts/dev.sh run` trong phiên GNOME X11 và mở khóa GNOME Keyring nếu hệ điều hành yêu cầu. Sao chép dấu vân tay ở thẻ thiết bị. Đối chiếu **đầy đủ 64 ký tự hex** qua kênh tin cậy; dấu vân tay là thông tin công khai, khóa riêng không được trao đổi.

Máy chia sẻ bấm **Lấy IP + cổng** ngay cạnh **Chia sẻ màn hình máy này** trên màn hình chính; hộp thoại mở ra và tự lấy địa chỉ. Có thể mở bằng nút chia sẻ rồi bấm nút lấy IP bên trong hộp thoại. Nút lấy IPv4 từ các interface của chính máy, bỏ qua loopback và link-local, rồi điền cổng mặc định `4433`. Nếu đã sửa cổng hợp lệ, nút giữ cổng đó. Khi có nhiều IP, chọn mạng chung với máy xem; Ethernet/Wi-Fi được hiển thị trước các adapter container/VPN. Chức năng này không cần truy cập Internet và vẫn cho phép nhập IP/cổng thủ công, kể cả IPv6. Lấy địa chỉ không tự bật listener hay thay địa chỉ máy đích ở ô nhận ảnh.

Popup hiển thị riêng **Dấu vân tay máy này**. Bấm **Sao chép IP + vân tay** để lấy cả IP:cổng và dấu vân tay của chính máy này, gửi qua kênh tin cậy cho người kia. Nội dung có dạng:

```text
BeoDesk
IP: 192.168.1.20:4433
Dấu vân tay: <đầy đủ 64 ký tự hex, chia thành 8 nhóm>
```

Máy xem sao chép nội dung nhận được rồi bấm **Dán IP + vân tay** trên màn hình chính để áp dụng cả hai ô. Nút cũng nhận dấu vân tay thuần; trường hợp này giữ nguyên địa chỉ đang nhập. Trong popup host, **Dán vân tay máy xem** nhận cùng định dạng nhưng chỉ thay ô dấu vân tay máy xem, giữ nguyên IP:cổng của host. Nút sao chép luôn dùng dấu vân tay **máy này**, không lấy dấu vân tay người kia đang nhập trong ô peer. Clipboard trống, dấu vân tay thiếu hoặc địa chỉ không hợp lệ sẽ không được áp dụng một phần.

Nhập/dán dấu vân tay máy xem rồi bật chia sẻ. Máy xem chọn **Nhận ảnh màn hình**. Host nhận hộp thoại xác nhận chỉ sau khi controller đã chứng minh sở hữu khóa đúng. **Cho phép một ảnh** cấp quyền chụp một lần; **Từ chối** không gọi capture. Yêu cầu hết hạn sau 60 giây. Máy xem có thể hủy khi đang chờ. Sao chép/dán thông tin không tự bật listener hay tự chấp thuận một yêu cầu xem.

Giữ máy chia sẻ mở khóa. Ứng dụng kiểm tra `org.gnome.ScreenSaver.GetActive` khi bật listener, trước capture và sau capture. Nếu GNOME không trả lời hoặc báo đã khóa, không gửi ảnh. Chọn **Dừng chia sẻ** để đóng listener và các kết nối. Listener không tự bật lại khi mở ứng dụng; trust của peer chưa lưu giữa các lần chạy.

UDP tại cổng đã nhập phải đi được giữa hai máy. Ứng dụng không tự sửa firewall, NAT hoặc router và chưa có relay/TCP fallback. Địa chỉ phải là IP và port; IPv6 dùng dạng `[fd00::20]:4433`. Không nhập hostname, Device ID hay địa chỉ bind wildcard. IPv6 chưa được nghiệm thu qua hai máy thật.

## Khi báo lỗi kết nối

Nếu báo `connection lost`, nguyên nhân có thể nằm ở phần xác thực hoặc máy chia sẻ đã đóng kết nối. Máy xem nhập đúng dấu vân tay host vẫn chưa đủ: host phải nhập đúng dấu vân tay của **máy xem** khi bật chia sẻ. Sao chép từ nút **Sao chép** trên máy xem, dừng chia sẻ ở host rồi bật lại với IP LAN cụ thể và dấu vân tay vừa đối chiếu. Không dùng `127.0.0.1` để nhận kết nối từ máy khác.

Nếu Chi tiết chứa cả `aborted by peer` và `Peer device fingerprint does not match the verified pin`, **máy chia sẻ đang từ chối dấu vân tay máy xem**; lỗi xảy ra trước hộp thoại xin phép. Máy xem hiện hướng dẫn cùng nút **Sao chép vân tay máy xem**. Gửi dấu vân tay này qua kênh tin cậy sang máy chia sẻ, chọn **Dừng chia sẻ**, mở **Chia sẻ màn hình máy này**, dán vào ô **Dấu vân tay của máy xem**, đối chiếu đầy đủ rồi **Bật chia sẻ**. Sau đó máy xem bấm **Nhận ảnh màn hình** lần nữa. Khi chia sẻ đang bật, giao diện hiện dấu vân tay máy xem đã cấu hình để dễ đối chiếu. Không dùng dấu vân tay của chính host trong ô máy xem khi kết nối hai máy khác nhau.

Nếu lỗi pin nằm ở bước `Authenticating LAN host at ...` và không có `aborted by peer`, máy xem đang từ chối dấu vân tay host. Sao chép lại **IP + vân tay** từ máy chia sẻ, đối chiếu đầy đủ qua kênh tin cậy rồi dán vào form nhận ảnh. Ứng dụng không tự thay dấu vân tay đã tin cậy hoặc bỏ kiểm tra xác thực khi lỗi.

Bản mới có nút **Chi tiết** ở thông báo lỗi, giữ cả nguyên nhân Rust/TLS bên dưới dòng đầu. Khi chạy `./beodesk` từ terminal trên host, lỗi xác thực cũng xuất hiện dưới tiền tố `BeoDesk LAN peer authentication`. Các log này chỉ chứa thông tin lỗi, không có khóa riêng. Lỗi pin không khớp được tái hiện bằng kiểm thử QUIC thật; ảnh chụp chỉ có dòng `connection lost` chưa đủ để xác định chắc chắn nguyên nhân của một kết nối cụ thể.

Nếu báo timeout hoặc connection refused, kiểm tra host đang hiện **Đang chờ yêu cầu tại ...**, địa chỉ/cổng đúng, hai máy đi được UDP và firewall không chặn cổng. Nếu xác thực thành công nhưng host báo không thể capture, kiểm tra GNOME X11, desktop mở khóa và giới hạn ảnh ở phần dưới.

## Giới hạn

- Host chỉ hỗ trợ Ubuntu GNOME X11. Windows capture và Wayland portal chưa có; Android chưa có secure storage nên chưa dùng làm controller được.
- Capture toàn bộ root desktop X11, chưa chọn monitor. Tổng số pixel tối đa `3840 × 2160`; desktop ghép nhiều màn hình vượt giới hạn sẽ bị từ chối.
- Một PNG RGBA8 tối đa 8 MiB, truyền trên reliable QUIC stream. Ảnh phức tạp vượt giới hạn nén hoặc capture vượt lease 2 giây sẽ không được gửi.
- Không có video liên tục, H.264, chuột/phím, đồng bộ clipboard giữa hai máy, file transfer, unattended access hoặc device discovery. Clipboard cục bộ chỉ dùng cho nút sao chép/dán thông tin kết nối.
- Mỗi host xử lý tuần tự một yêu cầu; một viewer chỉ có một yêu cầu đang chạy. Tắt ứng dụng sẽ kết thúc listener, không có dịch vụ nền.
- Giao diện chỉ giữ ảnh nhận trong bộ nhớ. Riêng bài kiểm tra `snapshot-smoke` ghi ảnh của màn hình ảo vào `.local/lan-snapshot.png`.

## Kiểm tra đã thực hiện

Tại Ubuntu 22.04.5 x64 / GNOME X11, ngày 2026-10-01–02:

- 26 Rust tests đạt, gồm 9 bài UDP QUIC thực tế; sai pin ở cả hai phía bị từ chối, gói khai báo quá lớn bị chặn trước khi đọc body, từ chối/ngắt khi đang chờ không gọi capture. Lỗi do host nhập sai pin viewer giữ được nguyên nhân xác thực; kiểm thử phân biệt lỗi host từ chối viewer (`aborted by peer`) với viewer từ chối host (`Authenticating LAN host at ...`).
- 22 Flutter tests đạt: 19 widget tests cho giao diện, quyền phiên, sao chép/dán đúng vai trò, hướng dẫn đúng phía lỗi pin và hiển thị pin máy xem sau khi bật lại chia sẻ; 3 tests kiểm tra định dạng thông tin kết nối, IPv6 và từ chối dữ liệu không hợp lệ. Clipboard trong widget tests được giả lập, không đọc/sửa clipboard thật của người dùng.
- Bài loopback dùng định danh GNOME Keyring thật, màn hình Xvfb 1280×720 và xác nhận do chính test thực hiện đã nhận PNG hợp lệ.
- Native Flutter integration trên Xvfb đã lấy IP thật của chính máy bằng nút mới, bật host ở địa chỉ đó, gửi yêu cầu qua QUIC từ cùng máy, xác nhận bằng hộp thoại và hiển thị PNG qua bridge thật. Kiểm tra trạng thái tải kết thúc khi ảnh xuất hiện đã đạt. Bài này vẫn chỉ chạy trên một máy.

Chạy `bash scripts/dev.sh lan-smoke` để lặp lại bài giao diện đầu cuối, hoặc `bash scripts/dev.sh snapshot-smoke` để kiểm tra luồng Rust/capture. Cả hai cần GNOME Keyring và desktop thật đang mở khóa, nhưng chỉ chụp màn hình ảo của bài kiểm tra.

Đây là kiểm tra trên một máy và màn hình ảo. Chưa có số đo độ trễ/fps hoặc nghiệm thu LAN hai máy, Windows, Android runtime. Hướng dẫn build nằm ở [development.md](development.md).
