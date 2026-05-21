using Xunit;

namespace Example.Tests;

public class ProgramTests
{
    [Fact]
    public void Adds()
    {
        Assert.Equal(5, Example.Program.Add(2, 3));
    }
}
