const std = @import("std");

pub fn add(a: i64, b: i64) i64 {
    return a + b;
}

test "adds" {
    try std.testing.expectEqual(@as(i64, 5), add(2, 3));
}
