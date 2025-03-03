use std::sync::Arc;
use rodio::{OutputStream, Sink};
use slint::PhysicalPosition;
use slint::VecModel;
use slint::Model;
slint::include_modules!();

mod music {
    use std::fs::File;
    use std::io::BufReader;
    use std::path::Path;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;
    use rodio::{Decoder, Sink, Source};

    pub struct SinkWrapper {
        sink: Arc<Sink>
    }

    impl SinkWrapper {

        pub fn new(sink: Sink) -> Self {
            SinkWrapper {
                sink: Arc::new(sink)
            }
        }

        pub fn play<P: AsRef<Path>, F>(&self, file_path: P, callback: F)
            where
                F: Fn(i32)
        {
            let sink = self.sink.clone();
            let file = File::open(file_path).unwrap();
            let source = Decoder::new(BufReader::new(file)).expect("解码音频失败");
            let song_duration = source.total_duration().unwrap().as_secs_f32() as i32;
            callback(song_duration);

            thread:: spawn(move || {
                sink.stop();            // 停止当前播放
                sink.append(source);    // 将音频源追加到播放队列
                sink.sleep_until_end(); // 阻塞线程直到播放完成
            });

        }

        pub fn stop(&self) {
            let sink = self.sink.clone();

            let _ = thread::spawn(move || {
                sink.stop();
            });
        }

        pub fn try_seek(&self, pos: Duration) {
            let sink = self.sink.clone();

            let _ = thread::spawn(move || {
                sink.try_seek(pos).unwrap();
            });
        }
    }
}

use music::SinkWrapper;

fn main() {
    let app = App::new().unwrap();

    // 创建默认音频输出流和流句柄，用于音频播放
    let (_stream, stream_handle) = OutputStream::try_default().unwrap();
    // 创建音频播放 Sink 对象
    let sink = Sink::try_new(&stream_handle).unwrap();
    // 将 Sink 包装为线程安全的 SinkWrapper，并使用 Arc 共享
    let sw = Arc::new(SinkWrapper::new(sink));

    // 获取应用的弱引用，用于在闭包中安全访问应用实例
    let app_weak = app.as_weak();
    // 设置窗口拖动事件处理
    app.on_drag_window(move |delta_x, delta_y| {
        let app = app_weak.unwrap();           // 从弱引用获取应用实例
        let window = app.window();             // 获取窗口对象
        let scale = window.scale_factor();     // 获取窗口缩放因子

        let position = window.position();      // 获取当前窗口位置
        // 根据拖动偏移和缩放因子计算新位置
        let x = position.x + (delta_x * scale) as i32;
        let y = position.y + (delta_y * scale) as i32;

        // 设置窗口的新位置
        window.set_position(PhysicalPosition::new(x, y));
    });

    // 设置 PlaySongsStore 的清空事件回调
    let app_weak = app.as_weak();
    let sw_clone = sw.clone();  // 克隆 SinkWrapper 用于线程安全访问
    app.global::<PlaySongsStore>().on_clear(move || {
        println!("全部清除");

        // 获取播放列表数据模型
        let songs_rc = app_weak.unwrap().global::<PlaySongsStore>().get_list();
        let songs_model = songs_rc.as_any().downcast_ref::<VecModel<Song>>().unwrap();
        songs_model.clear();  // 清空播放列表
        sw_clone.stop();      // 停止音频播放
    });

    // 设置 PlaySongsStore 的添加歌曲事件回调
    let app_weak = app.as_weak();
    let sw_clone = sw.clone();
    app.global::<PlaySongsStore>().on_push(move |song| {
        let app_weak_clone = app_weak.clone();
        // 播放歌曲并通过回调设置歌曲时长
        sw_clone.play(song.path.as_str(), move |duration| {
            println!("{:?}", duration);
            app_weak_clone.unwrap().global::<PlaySongsStore>().set_song_duration(duration);
        });

        let songs_rc = app_weak.unwrap().global::<PlaySongsStore>().get_list();
        let mut index = -1;
        // 查找歌曲是否已存在于列表中
        let res = songs_rc.iter().find(|s| {
            index += 1;
            s.id == song.id
        });

        if res.is_some() {
            // 如果歌曲已存在，仅更新当前播放索引
            println!("只修改当前播放歌曲的索引");
            app_weak.unwrap().global::<PlaySongsStore>().set_current_index(index);
            return;
        }

        // 如果歌曲不存在，添加到列表并更新索引
        let songs_model = songs_rc.as_any().downcast_ref::<VecModel<Song>>().unwrap();
        songs_model.push(song);  // 添加新歌曲

        let size = app_weak.unwrap().global::<PlaySongsStore>().get_size();
        app_weak.unwrap().global::<PlaySongsStore>().set_current_index(size - 1);
        println!("添加");
    });

    // 设置 PlaySongsStore 的删除歌曲事件回调
    let app_weak = app.as_weak();
    let sw_clone = sw.clone();
    app.global::<PlaySongsStore>().on_delete(move |index| {
        let songs_rc = app_weak.unwrap().global::<PlaySongsStore>().get_list();
        let songs_model = songs_rc.as_any().downcast_ref::<VecModel<Song>>().unwrap();
        let size = app_weak.unwrap().global::<PlaySongsStore>().get_size();

        songs_model.remove(index as usize);  // 删除指定索引的歌曲

        let mut current_index = app_weak.unwrap().global::<PlaySongsStore>().get_current_index();
        // 调整当前播放索引
        if index < current_index {
            current_index -= 1;
        } else if index == size - 1 {
            if index == 0 {
                current_index = -1;
            } else {
                current_index = 0;
            }
        }
        app_weak.unwrap().global::<PlaySongsStore>().set_current_index(current_index);

        // 如果仍有歌曲，播放当前索引的歌曲，否则停止播放
        if current_index >= 0 {
            let song = songs_model.iter().nth(current_index as usize).unwrap();
            let app_weak_clone = app_weak.clone();
            sw_clone.play(song.path, move |duration| {
                app_weak_clone.unwrap().global::<PlaySongsStore>().set_song_duration(duration);
            });
        } else {
            sw_clone.stop();
        }
        println!("删除单条数据");
    });

    // 设置 PlaySongsStore 的上一首事件回调
    let app_weak = app.as_weak();
    let sw_clone = sw.clone();
    app.global::<PlaySongsStore>().on_pre(move || {
        let mut current_index = app_weak.unwrap().global::<PlaySongsStore>().get_current_index();
        let size = app_weak.unwrap().global::<PlaySongsStore>().get_size();

        // 计算上一首的索引（循环播放）
        if size <= 0 {
            current_index = -1;
        } else {
            if current_index > 0 {
                current_index -= 1;
            } else {
                current_index = size - 1;
            }
        }

        app_weak.unwrap().global::<PlaySongsStore>().set_current_index(current_index);
        println!("上一首");
        if current_index >= 0 {
            let songs_rc = app_weak.unwrap().global::<PlaySongsStore>().get_list();
            let song = songs_rc.iter().nth(current_index as usize).unwrap();
            let app_weak_clone = app_weak.clone();
            sw_clone.play(song.path, move |duration| {
                app_weak_clone.unwrap().global::<PlaySongsStore>().set_song_duration(duration);
            });
        }
    });

    // 设置 PlaySongsStore 的下一首事件回调
    let app_weak = app.as_weak();
    let sw_clone = sw.clone();
    app.global::<PlaySongsStore>().on_next(move || {
        let mut current_index = app_weak.unwrap().global::<PlaySongsStore>().get_current_index();
        let size = app_weak.unwrap().global::<PlaySongsStore>().get_size();

        // 计算下一首的索引（循环播放）
        if size <= 0 {
            current_index = -1;
        } else {
            if current_index < size - 1 {
                current_index += 1;
            } else {
                current_index = 0;
            }
        }

        app_weak.unwrap().global::<PlaySongsStore>().set_current_index(current_index);
        println!("下一首");
        if current_index >= 0 {
            let songs_rc = app_weak.unwrap().global::<PlaySongsStore>().get_list();
            let song = songs_rc.iter().nth(current_index as usize).unwrap();
            let app_weak_clone = app_weak.clone();
            sw_clone.play(song.path, move |duration| {
                app_weak_clone.unwrap().global::<PlaySongsStore>().set_song_duration(duration);
            });
        }
    });

    // 设置 WindowStore 的关闭事件回调
    app.global::<WindowStore>().on_close(|| {
        slint::quit_event_loop().unwrap();  // 退出事件循环，关闭应用
    });

    // 设置 WindowStore 的最小化事件回调
    let app_weak = app.as_weak();
    app.global::<WindowStore>().on_min(move || {
        let app = app_weak.unwrap();
        let window = app.window();
        window.set_minimized(true);  // 最小化窗口
    });

    // 设置 WindowStore 的最大化事件回调
    let app_weak = app.as_weak();
    app.global::<WindowStore>().on_max(move || {
        let app = app_weak.unwrap();
        app.global::<WindowStore>().set_app_width(1920.0);  // 设置最大化宽度
        app.global::<WindowStore>().set_app_height(1067.0); // 设置最大化高度
        app_weak.unwrap().window().set_position(PhysicalPosition::new(0, -30)); // 设置位置
    });

    // 设置 WindowStore 的重置事件回调
    let app_weak = app.as_weak();
    app.global::<WindowStore>().on_reset(move || {
        let app = app_weak.unwrap();
        app.global::<WindowStore>().set_app_width(1150.0);  // 恢复默认宽度
        app.global::<WindowStore>().set_app_height(720.0);  // 恢复默认高度
        app_weak.unwrap().window().set_position(PhysicalPosition::new(200, 200)); // 恢复默认位置
    });

    // 设置 HistoryStore 的添加页面事件回调
    let app_weak = app.as_weak();
    app.global::<HistoryStore>().on_push(move |page| {
        let app = app_weak.unwrap();
        let pages_rc = app.global::<HistoryStore>().get_pages();
        let pages_model = pages_rc.as_any().downcast_ref::<VecModel<Page>>().unwrap();
        pages_model.push(page);  // 将新页面添加到历史记录
    });

    // 设置 HistoryStore 的弹出页面事件回调
    let app_weak = app.as_weak();
    app.global::<HistoryStore>().on_pop(move || {
        let app = app_weak.unwrap();
        let pages_rc = app.global::<HistoryStore>().get_pages();
        let size = app.global::<HistoryStore>().get_size() as usize;
        let pages_model = pages_rc.as_any().downcast_ref::<VecModel<Page>>().unwrap();

        if size > 0 {
            pages_model.remove(size - 1);  // 移除最后一个页面
        }
    });

    // 运行 Slint 应用程序，启动事件循环
    app.run().unwrap();
}