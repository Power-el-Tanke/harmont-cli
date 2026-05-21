const std = @import("std");
const example = @import("root.zig");

pub fn main() !void {
    std.debug.print("{d}\n", .{example.add(2, 3)});
}
