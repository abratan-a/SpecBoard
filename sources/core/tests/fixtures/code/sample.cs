using System;

namespace Shop
{
    /// <summary>An order.</summary>
    [Serializable]
    public partial class Order : IPriced
    {
        /// <summary>The most an order holds.</summary>
        public const int Most = 10;

        private int[] lines;

        /// <summary>A new one.</summary>
        public Order() { lines = new int[0]; }

        public Order(int[] lines) { this.lines = lines; }

        public int Price() => Total() + Helper.Help(1);

        int Total()
        {
            int Local() => 0;
            return Local();
        }

        public int Count { get; set; }

        public struct Line { public int Count; }
    }

    public interface IPriced
    {
        int Price();
    }

    public enum State { Open, Shut }

    public record Point(int X, int Y);

    public delegate void Changed(object sender);
}

namespace Shop.Inner;
