const std = @import("std");

pub fn sub(a: i64, b: i64) i64 {
    return a - b;
}

test "subs" {
    try std.testing.expectEqual(@as(i64, 6), sub(10, 4));
}
