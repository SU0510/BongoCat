//! Minimal two-locale UI strings for InSync. The BongoCat i18n crate is
//! product-specific; InSync ships its own tiny catalog until it grows one.

pub fn text(language: &str, key: &str) -> &'static str {
    let zh = language.starts_with("zh");
    match key {
        "app.name" => "InSync",
        "tab.chat" => {
            if zh {
                "消息"
            } else {
                "Chat"
            }
        }
        "tab.letters" => {
            if zh {
                "留言"
            } else {
                "Letters"
            }
        }
        "tab.focus" => {
            if zh {
                "专注"
            } else {
                "Focus"
            }
        }
        "tab.todos" => {
            if zh {
                "待办"
            } else {
                "Todos"
            }
        }
        "tab.notes" => {
            if zh {
                "便签"
            } else {
                "Notes"
            }
        }
        "tab.anniversaries" => {
            if zh {
                "纪念日"
            } else {
                "Days"
            }
        }
        "tab.alarms" => {
            if zh {
                "闹钟"
            } else {
                "Alarms"
            }
        }
        "tab.settings" => {
            if zh {
                "设置"
            } else {
                "Settings"
            }
        }
        "tray.open" => {
            if zh {
                "打开面板"
            } else {
                "Open Panel"
            }
        }
        "tray.poke" => {
            if zh {
                "戳戳伴侣"
            } else {
                "Poke Partner"
            }
        }
        "tray.hide" => {
            if zh {
                "隐藏桌宠"
            } else {
                "Hide Pet"
            }
        }
        "tray.show" => {
            if zh {
                "显示桌宠"
            } else {
                "Show Pet"
            }
        }
        "tray.quit" => {
            if zh {
                "退出"
            } else {
                "Quit"
            }
        }
        "chat.send" => {
            if zh {
                "发送"
            } else {
                "Send"
            }
        }
        "chat.placeholder" => {
            if zh {
                "说点什么…"
            } else {
                "Say something…"
            }
        }
        "chat.empty" => {
            if zh {
                "还没有消息，说声你好吧"
            } else {
                "No messages yet. Say hi!"
            }
        }
        "letters.placeholder" => {
            if zh {
                "写下想对 TA 说的话…"
            } else {
                "Write a letter…"
            }
        }
        "letters.leave" => {
            if zh {
                "留下留言"
            } else {
                "Leave letter"
            }
        }
        "letters.empty" => {
            if zh {
                "还没有留言"
            } else {
                "No letters yet"
            }
        }
        "focus.invite" => {
            if zh {
                "发送邀请"
            } else {
                "Send invite"
            }
        }
        "focus.minutes" => {
            if zh {
                "分钟"
            } else {
                "min"
            }
        }
        "focus.topic" => {
            if zh {
                "主题（可选）"
            } else {
                "Topic (optional)"
            }
        }
        "focus.pause" => {
            if zh {
                "暂停"
            } else {
                "Pause"
            }
        }
        "focus.resume" => {
            if zh {
                "继续"
            } else {
                "Resume"
            }
        }
        "focus.finish" => {
            if zh {
                "提前结束"
            } else {
                "Finish early"
            }
        }
        "focus.waiting" => {
            if zh {
                "等待对方接受…"
            } else {
                "Waiting for partner…"
            }
        }
        "focus.cancel" => {
            if zh {
                "取消邀请"
            } else {
                "Cancel invite"
            }
        }
        "todos.add" => {
            if zh {
                "添加待办"
            } else {
                "Add todo"
            }
        }
        "todos.personal" => {
            if zh {
                "个人"
            } else {
                "Personal"
            }
        }
        "todos.shared" => {
            if zh {
                "共享"
            } else {
                "Shared"
            }
        }
        "notes.add" => {
            if zh {
                "新建便签"
            } else {
                "New note"
            }
        }
        "notes.personal" => {
            if zh {
                "个人"
            } else {
                "Personal"
            }
        }
        "notes.shared" => {
            if zh {
                "共享"
            } else {
                "Shared"
            }
        }
        "days.add" => {
            if zh {
                "添加纪念日"
            } else {
                "Add anniversary"
            }
        }
        "days.days_left" => {
            if zh {
                "天"
            } else {
                "days"
            }
        }
        "alarms.add" => {
            if zh {
                "新建闹钟"
            } else {
                "New alarm"
            }
        }
        "settings.server" => {
            if zh {
                "服务器地址"
            } else {
                "Server URL"
            }
        }
        "settings.account" => {
            if zh {
                "账号"
            } else {
                "Account"
            }
        }
        "settings.login" => {
            if zh {
                "登录"
            } else {
                "Sign in"
            }
        }
        "settings.register" => {
            if zh {
                "注册"
            } else {
                "Register"
            }
        }
        "settings.logout" => {
            if zh {
                "退出登录"
            } else {
                "Sign out"
            }
        }
        "settings.invite_code" => {
            if zh {
                "邀请码"
            } else {
                "Invite code"
            }
        }
        "settings.accept_invite" => {
            if zh {
                "接受邀请"
            } else {
                "Accept invite"
            }
        }
        "settings.dissolve" => {
            if zh {
                "解除绑定"
            } else {
                "Unbind"
            }
        }
        "settings.preferences" => {
            if zh {
                "偏好设置"
            } else {
                "Preferences"
            }
        }
        "settings.theme" => {
            if zh {
                "主题"
            } else {
                "Theme"
            }
        }
        "settings.partner" => {
            if zh {
                "伴侣"
            } else {
                "Partner"
            }
        }
        "settings.character" => {
            if zh {
                "桌宠角色"
            } else {
                "Pet character"
            }
        }
        "character.momo" => {
            if zh {
                "Momo（粉）"
            } else {
                "Momo (pink)"
            }
        }
        "character.nana" => {
            if zh {
                "Nana（麻花辫）"
            } else {
                "Nana (braids)"
            }
        }
        "settings.no_partner" => {
            if zh {
                "尚未绑定伴侣"
            } else {
                "No partner bound yet"
            }
        }
        "status.online" => {
            if zh {
                "在线"
            } else {
                "Online"
            }
        }
        "status.offline" => {
            if zh {
                "离线"
            } else {
                "Offline"
            }
        }
        "status.retrying" => {
            if zh {
                "重连中…"
            } else {
                "Reconnecting…"
            }
        }
        "status.relogin" => {
            if zh {
                "请重新登录"
            } else {
                "Please sign in again"
            }
        }
        "status.dnd" => {
            if zh {
                "勿扰"
            } else {
                "DND"
            }
        }
        "notify.new_message" => {
            if zh {
                "收到新消息"
            } else {
                "New message"
            }
        }
        "notify.new_letter" => {
            if zh {
                "收到留言"
            } else {
                "New letter"
            }
        }
        "notify.poked" => {
            if zh {
                "被戳了一下！"
            } else {
                "You were poked!"
            }
        }
        "notify.alarm" => {
            if zh {
                "闹钟"
            } else {
                "Alarm"
            }
        }
        "notify.focus_invite" => {
            if zh {
                "共同专注邀请"
            } else {
                "Focus invite"
            }
        }
        "notify.focus_done" => {
            if zh {
                "共同专注完成！"
            } else {
                "Focus complete!"
            }
        }
        "settings.username" => {
            if zh {
                "用户名"
            } else {
                "Username"
            }
        }
        "settings.password" => {
            if zh {
                "密码"
            } else {
                "Password"
            }
        }
        "settings.no_account" => {
            if zh {
                "还没有登录"
            } else {
                "Not signed in"
            }
        }
        "settings.local_hint" => {
            if zh {
                "个人待办、个人便签、纪念日和闹钟只保存在这台设备上，不会同步给对方。"
            } else {
                "Personal todos, notes, days and alarms stay on this device and are never sent to your partner."
            }
        }
        "alarms.time" => {
            if zh {
                "时间 09:30"
            } else {
                "Time 09:30"
            }
        }
        "chat.empty_hint" => {
            if zh {
                "说点什么，对方马上就收到"
            } else {
                "Say something and it lands right away"
            }
        }
        "letters.empty_hint" => {
            if zh {
                "写几句悄悄话，对方下次打开就能看到"
            } else {
                "Leave a few words for your partner to find later"
            }
        }
        "letters.from_me" => {
            if zh {
                "我写下的"
            } else {
                "From me"
            }
        }
        "letters.from_partner" => {
            if zh {
                "对方写下的"
            } else {
                "From my partner"
            }
        }
        "letters.unread" => {
            if zh {
                "新"
            } else {
                "New"
            }
        }
        "letters.mark_read" => {
            if zh {
                "读过啦"
            } else {
                "Mark as read"
            }
        }
        "focus.start" => {
            if zh {
                "开始专注"
            } else {
                "Start focus"
            }
        }
        "focus.hint" => {
            if zh {
                "邀请伴侣一起专注，时间到了两边都会收到提醒"
            } else {
                "Invite your partner to focus too — you both get a nudge when time is up"
            }
        }
        "focus.accept" => {
            if zh {
                "一起专注"
            } else {
                "Join"
            }
        }
        "focus.decline" => {
            if zh {
                "下次吧"
            } else {
                "Not now"
            }
        }
        "focus.invited" => {
            if zh {
                "对方邀请你一起专注"
            } else {
                "Your partner invited you to focus"
            }
        }
        "focus.active" => {
            if zh {
                "正在专注"
            } else {
                "Focusing"
            }
        }
        "focus.paused" => {
            if zh {
                "暂停中"
            } else {
                "Paused"
            }
        }
        "focus.waiting_partner" => {
            if zh {
                "等对方接受邀请…"
            } else {
                "Waiting for your partner…"
            }
        }
        "todos.empty" => {
            if zh {
                "还没有待办"
            } else {
                "Nothing on the list"
            }
        }
        "todos.empty_hint" => {
            if zh {
                "写下第一件要做的事"
            } else {
                "Add the first thing to do"
            }
        }
        "todos.add_shared" => {
            if zh {
                "添加共同待办"
            } else {
                "Add a shared todo"
            }
        }
        "notes.empty" => {
            if zh {
                "还没有便签"
            } else {
                "No notes yet"
            }
        }
        "notes.empty_hint" => {
            if zh {
                "记下想留住的小事"
            } else {
                "Jot down something worth keeping"
            }
        }
        "notes.new_shared" => {
            if zh {
                "新建共同便签"
            } else {
                "New shared note"
            }
        }
        "notes.save_personal" => {
            if zh {
                "保存"
            } else {
                "Save"
            }
        }
        "notes.save_shared" => {
            if zh {
                "保存并分享"
            } else {
                "Save and share"
            }
        }
        "days.personal" => {
            if zh {
                "我的纪念日"
            } else {
                "My days"
            }
        }
        "days.shared" => {
            if zh {
                "我们的纪念日"
            } else {
                "Our days"
            }
        }
        "days.empty" => {
            if zh {
                "还没有纪念日"
            } else {
                "No days yet"
            }
        }
        "days.empty_hint" => {
            if zh {
                "记下在一起的日子"
            } else {
                "Remember a day that matters"
            }
        }
        "days.add_personal" => {
            if zh {
                "添加纪念日"
            } else {
                "Add a day"
            }
        }
        "days.add_shared" => {
            if zh {
                "添加共同纪念日"
            } else {
                "Add a shared day"
            }
        }
        "days.format_hint" => {
            if zh {
                "日期格式 2000-01-31"
            } else {
                "Date format 2000-01-31"
            }
        }
        "days.in" => {
            if zh {
                "还有"
            } else {
                "in"
            }
        }
        "days.ago" => {
            if zh {
                "天前"
            } else {
                "days ago"
            }
        }
        "alarms.empty" => {
            if zh {
                "还没有闹钟"
            } else {
                "No alarms yet"
            }
        }
        "alarms.empty_hint" => {
            if zh {
                "设一个时间，到点宠物会叫你"
            } else {
                "Set a time and the pet will call you"
            }
        }
        "alarms.enable" => {
            if zh {
                "打开"
            } else {
                "Enable"
            }
        }
        "alarms.disable" => {
            if zh {
                "关闭"
            } else {
                "Disable"
            }
        }
        "alarms.once" => {
            if zh {
                "仅一次"
            } else {
                "Once on"
            }
        }
        "alarms.daily" => {
            if zh {
                "每天"
            } else {
                "Daily"
            }
        }
        "alarms.next" => {
            if zh {
                "下次"
            } else {
                "next"
            }
        }
        "hub.delete" => {
            if zh {
                "删除"
            } else {
                "Delete"
            }
        }
        "weekday.0" => {
            if zh {
                "周日"
            } else {
                "Sun"
            }
        }
        "weekday.1" => {
            if zh {
                "周一"
            } else {
                "Mon"
            }
        }
        "weekday.2" => {
            if zh {
                "周二"
            } else {
                "Tue"
            }
        }
        "weekday.3" => {
            if zh {
                "周三"
            } else {
                "Wed"
            }
        }
        "weekday.4" => {
            if zh {
                "周四"
            } else {
                "Thu"
            }
        }
        "weekday.5" => {
            if zh {
                "周五"
            } else {
                "Fri"
            }
        }
        "weekday.6" => {
            if zh {
                "周六"
            } else {
                "Sat"
            }
        }
        _ => "",
    }
}
