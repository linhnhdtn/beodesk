#include "video_texture.h"
#include <dlfcn.h>
#include <cstring>
#include <map>

// Must match remote_bridge::video_texture::VideoPixels (repr(C)).
struct VideoPixels {
  const uint8_t* data;
  uint32_t width, height, generation;
  const void* handle;
};
using Acquire = bool (*)(uint32_t, VideoPixels*);
using Release = void (*)(const void*);
using Generation = uint32_t (*)(uint32_t);
struct _BeoTexture {
  FlPixelBufferTexture parent_instance;
  uint32_t session;
  VideoPixels pixels;
  Acquire acquire;
  Release release;
  // GLib atomic access: raster thread writes, platform thread reads.
  gint rendered;
};
struct BeoTextureClass { FlPixelBufferTextureClass parent_class; };
using BeoTexture = _BeoTexture;
G_DEFINE_TYPE(BeoTexture, beo_texture, fl_pixel_buffer_texture_get_type())

static gboolean copy_pixels(FlPixelBufferTexture* base, const uint8_t** buffer,
                            uint32_t* width, uint32_t* height, GError**) {
  auto* self = reinterpret_cast<BeoTexture*>(base);
  VideoPixels next{};
  if (self->acquire(self->session, &next)) {
    if (next.generation != self->pixels.generation)
      g_atomic_int_inc(&self->rendered);
    self->release(self->pixels.handle);
    self->pixels = next;
  }
  if (!self->pixels.handle) return FALSE;
  *buffer = self->pixels.data;
  *width = self->pixels.width;
  *height = self->pixels.height;
  return TRUE;
}
static void texture_finalize(GObject* object) {
  auto* self = reinterpret_cast<BeoTexture*>(object);
  if (self->release) self->release(self->pixels.handle);
  G_OBJECT_CLASS(beo_texture_parent_class)->finalize(object);
}
static void beo_texture_class_init(BeoTextureClass* klass) {
  FL_PIXEL_BUFFER_TEXTURE_CLASS(klass)->copy_pixels = copy_pixels;
  G_OBJECT_CLASS(klass)->finalize = texture_finalize;
}
static void beo_texture_init(BeoTexture*) {}

struct Entry { BeoTexture* texture; uint32_t marked; };
struct VideoManager {
  FlTextureRegistrar* registrar;
  FlMethodChannel* channel;
  guint timer = 0;
  // Keep the module loaded for the process lifetime: raster callbacks may outlive
  // the view's platform channel during engine shutdown.
  void* library = nullptr;
  Acquire acquire = nullptr;
  Release release = nullptr;
  Generation generation = nullptr;
  std::map<int64_t, Entry> textures;
  bool load() {
    if (library) return acquire && release && generation;
    library = dlopen("libremote_bridge.so", RTLD_NOW | RTLD_LOCAL);
    if (!library) return false;
    acquire = reinterpret_cast<Acquire>(dlsym(library, "beodesk_video_acquire"));
    release = reinterpret_cast<Release>(dlsym(library, "beodesk_video_release"));
    generation = reinterpret_cast<Generation>(dlsym(library, "beodesk_video_generation"));
    return acquire && release && generation;
  }
  ~VideoManager() {
    if (timer) g_source_remove(timer);
    fl_method_channel_set_method_call_handler(channel, nullptr, nullptr, nullptr);
    for (auto& pair : textures) {
      fl_texture_registrar_unregister_texture(registrar, FL_TEXTURE(pair.second.texture));
      g_object_unref(pair.second.texture);
    }
    g_object_unref(channel);
    g_object_unref(registrar);
  }
};
static gboolean tick(gpointer data) {
  auto* self = static_cast<VideoManager*>(data);
  for (auto& pair : self->textures) {
    auto& entry = pair.second;
    uint32_t generation = self->generation(entry.texture->session);
    if (generation && generation != entry.marked) {
      entry.marked = generation;
      fl_texture_registrar_mark_texture_frame_available(self->registrar, FL_TEXTURE(entry.texture));
    }
  }
  return G_SOURCE_CONTINUE;
}
static void method_call(FlMethodChannel*, FlMethodCall* call, gpointer data) {
  auto* self = static_cast<VideoManager*>(data);
  const char* method = fl_method_call_get_name(call);
  FlValue* args = fl_method_call_get_args(call);
  if (!args || fl_value_get_type(args) != FL_VALUE_TYPE_INT) {
    fl_method_call_respond_error(call, "arguments", "Expected session or texture ID", nullptr, nullptr);
    return;
  }
  int64_t id = fl_value_get_int(args);
  if (strcmp(method, "create") == 0) {
    if (id <= 0 || id > UINT32_MAX || !self->textures.empty() || !self->load()) {
      fl_method_call_respond_error(call, "video", "Native video unavailable or already active", nullptr, nullptr);
      return;
    }
    auto* texture = reinterpret_cast<BeoTexture*>(g_object_new(beo_texture_get_type(), nullptr));
    texture->session = static_cast<uint32_t>(id);
    texture->acquire = self->acquire;
    texture->release = self->release;
    if (!fl_texture_registrar_register_texture(self->registrar, FL_TEXTURE(texture))) {
      g_object_unref(texture);
      fl_method_call_respond_error(call, "video", "Cannot register video texture", nullptr, nullptr);
      return;
    }
    int64_t texture_id = fl_texture_get_id(FL_TEXTURE(texture));
    self->textures.emplace(texture_id, Entry{texture, 0});
    g_autoptr(FlValue) result = fl_value_new_int(texture_id);
    fl_method_call_respond_success(call, result, nullptr);
  } else if (strcmp(method, "dispose") == 0) {
    auto entry = self->textures.find(id);
    if (entry != self->textures.end()) {
      fl_texture_registrar_unregister_texture(self->registrar, FL_TEXTURE(entry->second.texture));
      g_object_unref(entry->second.texture);
      self->textures.erase(entry);
    }
    fl_method_call_respond_success(call, nullptr, nullptr);
  } else if (strcmp(method, "renderedFrames") == 0) {
    auto entry = self->textures.find(id);
    int count = entry == self->textures.end() ? 0 : g_atomic_int_get(&entry->second.texture->rendered);
    g_autoptr(FlValue) result = fl_value_new_int(count);
    fl_method_call_respond_success(call, result, nullptr);
  } else {
    fl_method_call_respond_not_implemented(call, nullptr);
  }
}
void beodesk_video_register(FlView* view) {
  auto* self = new VideoManager;
  FlEngine* engine = fl_view_get_engine(view);
  self->registrar = FL_TEXTURE_REGISTRAR(g_object_ref(fl_engine_get_texture_registrar(engine)));
  g_autoptr(FlStandardMethodCodec) codec = fl_standard_method_codec_new();
  self->channel = fl_method_channel_new(fl_engine_get_binary_messenger(engine), "beodesk/video", FL_METHOD_CODEC(codec));
  fl_method_channel_set_method_call_handler(self->channel, method_call, self, nullptr);
  self->timer = g_timeout_add(16, tick, self);
  // Gtk destroy runs before engine teardown; detach callbacks while the registrar
  // still exists. GObject references retain textures during in-flight rendering.
  g_signal_connect(view, "destroy", G_CALLBACK(+[](GtkWidget*, gpointer data) {
    delete static_cast<VideoManager*>(data);
  }), self);
}
