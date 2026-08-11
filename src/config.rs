pub const ADMIN_LOGIN_URL: &str = "https://donate.cherry-rush.org/admin/login/";
pub const COOKIE_FILE: &str = "cookie.txt";

pub const SOCKETS: &[(&str, &str)] = &[(
    "donations",
    "wss://donate.cherry-rush.org/tracker/ws/donations/",
)];
