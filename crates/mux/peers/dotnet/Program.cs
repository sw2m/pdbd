using System.Diagnostics;
using System.Globalization;
using System.Net;
using System.Net.Sockets;
using Nerdbank.Streams;

var version = typeof(MultiplexingStream).Assembly
    .GetCustomAttributes(typeof(System.Reflection.AssemblyInformationalVersionAttribute), false)
    .OfType<System.Reflection.AssemblyInformationalVersionAttribute>()
    .FirstOrDefault()?.InformationalVersion.Split('+')[0] ?? "?";
var name = $"Nerdbank.Streams {version}, v3 (c#)";
const int Runs = 5; // see crates/mux/CLAUDE.md §"Benchmarks": every harness runs these same numbers

for (var i = 0; i <= Runs; i++)
{
    var keep = i > 0;
    void Report(string key, double v)
    {
        if (keep)
        {
            Console.WriteLine($"{name}\t{key}\t{v.ToString("R", CultureInfo.InvariantCulture)}");
        }
    }
    Report("bulk", await Run.Measure(r => r.Bulk()));
    foreach (var n in new[] { 16, 256 })
    {
        await using var s = await Session.NewAsync();
        var (rate, spread) = await new Run(s).Flows(n);
        Report($"flows{n}", rate);
        Report($"fair{n}", spread);
    }
    Report("pingpong", await Run.Measure(r => r.Pingpong()));
    Report("churn", await Run.Measure(r => r.Churn()));
    Report("idle", await Run.Measure(r => r.Idle()));
}

sealed class Session : IAsyncDisposable
{
    public const int Window = 256 * 1024;

    readonly MultiplexingStream near;
    readonly MultiplexingStream far;
    int opened;
    int accepted;

    Session(MultiplexingStream near, MultiplexingStream far)
    {
        this.near = near;
        this.far = far;
    }

    public static async Task<Session> NewAsync()
    {
        var listener = new TcpListener(IPAddress.Loopback, 0);
        listener.Start();
        var dial = new TcpClient { NoDelay = true };
        var connect = dial.ConnectAsync(IPAddress.Loopback, ((IPEndPoint)listener.LocalEndpoint).Port);
        var answer = await listener.AcceptTcpClientAsync();
        await connect;
        listener.Stop();
        answer.NoDelay = true;
        var options = new MultiplexingStream.Options
        {
            ProtocolMajorVersion = 3,
            DefaultChannelReceivingWindowSize = Window,
        };
        var near = MultiplexingStream.CreateAsync(dial.GetStream(), options);
        var far = MultiplexingStream.CreateAsync(answer.GetStream(), options);
        return new Session(await near, await far);
    }

    public Task<MultiplexingStream.Channel> Offer() =>
        this.near.OfferChannelAsync(Interlocked.Increment(ref this.opened).ToString(CultureInfo.InvariantCulture));

    public Task<MultiplexingStream.Channel> Take() =>
        this.far.AcceptChannelAsync(Interlocked.Increment(ref this.accepted).ToString(CultureInfo.InvariantCulture));

    public async Task<Stream> Open() => (await this.Offer()).AsStream();

    public async Task<Stream> Accept() => (await this.Take()).AsStream();

    public async ValueTask DisposeAsync()
    {
        await this.near.DisposeAsync();
        await this.far.DisposeAsync();
    }
}

sealed class Run(Session s)
{
    const int Chunk = 64 * 1024;
    const int Volume = 64 << 20;
    const int Msg = 64;
    const int Warm = 100;
    const int Pings = 2000;
    const int Churns = 1000;
    const int Idles = 1000;

    static readonly byte[] Data = new byte[Chunk];

    public static async Task<double> Measure(Func<Run, Task<double>> scenario)
    {
        await using var s = await Session.NewAsync();
        return await scenario(new Run(s));
    }

    static double Rate(long bytes, TimeSpan spent) => bytes / (double)(1 << 20) / spent.TotalSeconds;

    static double Spread(double[] rates)
    {
        var mean = rates.Average();
        var v = rates.Select(r => (r - mean) * (r - mean)).Average();
        return 100 * Math.Sqrt(v) / mean;
    }

    static async Task<long> Drain(Stream f, long want)
    {
        var buf = new byte[Chunk];
        long got = 0;
        while (got < want)
        {
            var n = await f.ReadAsync(buf);
            if (n == 0)
            {
                throw new EndOfStreamException($"EOF after {got} of {want}");
            }
            got += n;
        }
        return Stopwatch.GetTimestamp();
    }

    static async Task Fill(Stream f, long want)
    {
        for (long sent = 0; sent < want;)
        {
            var n = (int)Math.Min(Chunk, want - sent);
            await f.WriteAsync(Data.AsMemory(0, n));
            sent += n;
        }
        await f.FlushAsync();
    }

    static async Task Exact(Stream f, byte[] buf)
    {
        for (var got = 0; got < buf.Length;)
        {
            var n = await f.ReadAsync(buf.AsMemory(got));
            if (n == 0)
            {
                throw new EndOfStreamException();
            }
            got += n;
        }
    }

    static async Task Finish(MultiplexingStream.Channel c)
    {
        await c.Output.CompleteAsync();
        while (true)
        {
            var r = await c.Input.ReadAsync();
            c.Input.AdvanceTo(r.Buffer.End);
            if (r.IsCompleted)
            {
                break;
            }
        }
        c.Dispose();
    }

    static async Task Send(Stream f, ReadOnlyMemory<byte> b)
    {
        await f.WriteAsync(b);
        await f.FlushAsync();
    }

    public async Task<double> Bulk()
    {
        var start = Stopwatch.GetTimestamp();
        var server = Task.Run(async () => await Drain(await s.Accept(), Volume));
        var near = await s.Open();
        await Fill(near, Volume);
        var end = await server;
        near.Dispose();
        return Rate(Volume, Stopwatch.GetElapsedTime(start, end));
    }

    public async Task<(double, double)> Flows(int n)
    {
        var share = Volume / n;
        var accepting = Task.Run(async () =>
        {
            var fs = new Stream[n];
            for (var i = 0; i < n; i++)
            {
                fs[i] = await s.Accept();
                await Exact(fs[i], new byte[1]);
            }
            return fs;
        });
        var outs = new Stream[n];
        for (var i = 0; i < n; i++)
        {
            outs[i] = await s.Open();
            await Send(outs[i], Data.AsMemory(0, 1));
        }
        var ins = await accepting;
        var start = Stopwatch.GetTimestamp();
        var readers = ins.Select(f => Task.Run(() => Drain(f, share))).ToArray();
        var writers = outs.Select(f => Task.Run(() => Fill(f, share))).ToArray();
        var ends = await Task.WhenAll(readers);
        await Task.WhenAll(writers);
        var rates = ends.Select(e => Rate(share, Stopwatch.GetElapsedTime(start, e))).ToArray();
        foreach (var f in outs.Concat(ins))
        {
            f.Dispose();
        }
        return (Rate((long)share * n, Stopwatch.GetElapsedTime(start, ends.Max())), Spread(rates));
    }

    public async Task<double> Pingpong()
    {
        var accepting = s.Accept();
        var near = await s.Open();
        var far = await accepting;
        var echo = Task.Run(async () =>
        {
            var b = new byte[Msg];
            try
            {
                while (true)
                {
                    await Exact(far, b);
                    await Send(far, b);
                }
            }
            catch (Exception)
            {
            }
        });
        var buf = new byte[Msg];
        var trips = new List<double>(Pings);
        for (var i = 0; i < Warm + Pings; i++)
        {
            var start = Stopwatch.GetTimestamp();
            await Send(near, Data.AsMemory(0, Msg));
            await Exact(near, buf);
            if (i >= Warm)
            {
                trips.Add(Stopwatch.GetElapsedTime(start).TotalMicroseconds);
            }
        }
        near.Dispose();
        far.Dispose();
        trips.Sort();
        return trips[trips.Count / 2];
    }

    public async Task<double> Churn()
    {
        var start = Stopwatch.GetTimestamp();
        var server = Task.Run(async () =>
        {
            var tasks = new List<Task>(Churns);
            for (var i = 0; i < Churns; i++)
            {
                var c = await s.Take();
                tasks.Add(Task.Run(async () =>
                {
                    var f = c.AsStream();
                    var one = new byte[1];
                    await Exact(f, one);
                    await Send(f, one);
                    await Finish(c);
                }));
            }
            await Task.WhenAll(tasks);
        });
        var b = new byte[1];
        for (var i = 0; i < Churns; i++)
        {
            var c = await s.Offer();
            var f = c.AsStream();
            await Send(f, Data.AsMemory(0, 1));
            await Exact(f, b);
            await Finish(c);
        }
        await server;
        return Churns / Stopwatch.GetElapsedTime(start).TotalSeconds;
    }

    public async Task<double> Idle()
    {
        var before = GC.GetTotalMemory(true);
        var server = Task.Run(async () =>
        {
            var fs = new List<Stream>(Idles);
            var one = new byte[1];
            for (var i = 0; i < Idles; i++)
            {
                var f = await s.Accept();
                await Exact(f, one);
                await Send(f, one);
                fs.Add(f);
            }
            return fs;
        });
        var outs = new List<Stream>(Idles);
        var b = new byte[1];
        for (var i = 0; i < Idles; i++)
        {
            var f = await s.Open();
            await Send(f, Data.AsMemory(0, 1));
            await Exact(f, b);
            outs.Add(f);
        }
        var ins = await server;
        await Task.Delay(50); // let the sessions settle their buffers
        var per = (GC.GetTotalMemory(true) - before) / (double)Idles;
        foreach (var f in outs.Concat(ins))
        {
            f.Dispose();
        }
        return per;
    }
}
