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
        "settings.language" => {
            if zh {
                "语言"
            } else {
                "Language"
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
        _ => "",
    }
}
