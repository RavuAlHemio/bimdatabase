use std::fmt::Write;


pub(crate) fn url<S: AsRef<str>>(string: S, _runtime_values: &dyn askama::Values) -> askama::Result<String, askama::Error> {
    let s = string.as_ref();
    let mut ret = String::with_capacity(s.len());
    for c in s.chars() {
        // RFC3986: unreserved = ALPHA / DIGIT / "-" / "." / "_" / "~"
        let is_unreserved =
            (c >= 'A' && c <= 'Z')
            || (c >= 'a' && c <= 'z')
            || (c >= '0' && c <= '9')
            || c == '-'
            || c == '.'
            || c == '_'
            || c == '~'
        ;
        if is_unreserved {
            ret.push(c);
        } else {
            let mut buf = [0u8; 4];
            let buf_slice = c.encode_utf8(&mut buf);
            for b in buf_slice.bytes() {
                write!(ret, "%{:02X}", b).expect("failed to write");
            }
        }
    }
    Ok(ret)
}
