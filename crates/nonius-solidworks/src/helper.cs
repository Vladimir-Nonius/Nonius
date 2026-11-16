// Помощник рабочего процесса (worker.ps1): то, чего PowerShell сам не умеет.
//
// - Wire  — строки ответа в UTF-8 мимо консоли (пути с кириллицей) и так,
//           чтобы две нити не перемешали свои строки;
// - Own   — свой экземпляр SolidWorks: запуск SLDWORKS.exe, поиск COM-объекта
//           именно этого процесса в таблице запущенных объектов Windows (ROT)
//           по имени SolidWorks_PID_<pid>, группа процессов (Job Object),
//           которую Windows закрывает вместе с рабочим процессом;
// - Watch — две сторожевые нити: «пульс» (время процессора своего SolidWorks
//           и заголовок его окна-сообщения, если оно появилось) и слежение
//           за Nonius: его не стало — свой SolidWorks закрывается.
//
// Почему нельзя Marshal.GetActiveObject("SldWorks.Application"): он отдаёт
// первый зарегистрированный SolidWorks — возможно, сеанс человека.
//
// Компилирует это сам PowerShell (Add-Type), компилятором .NET Framework:
// язык — C# 5, без новых удобств.
using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;
using System.Text;
using System.Threading;

namespace Nonius
{
    public static class Wire
    {
        static TextWriter output;
        static readonly object gate = new object();

        public static void Open()
        {
            output = new StreamWriter(Console.OpenStandardOutput(), new UTF8Encoding(false));
        }

        public static TextReader Input()
        {
            return new StreamReader(Console.OpenStandardInput(), new UTF8Encoding(false));
        }

        public static void Say(string line)
        {
            lock (gate)
            {
                output.Write(line.Replace("\r", " ").Replace("\n", " ") + "\n");
                output.Flush();
            }
        }
    }

    public static class Own
    {
        [StructLayout(LayoutKind.Sequential)]
        struct BasicLimits
        {
            public long PerProcessUserTimeLimit;
            public long PerJobUserTimeLimit;
            public uint LimitFlags;
            public UIntPtr MinimumWorkingSetSize;
            public UIntPtr MaximumWorkingSetSize;
            public uint ActiveProcessLimit;
            public UIntPtr Affinity;
            public uint PriorityClass;
            public uint SchedulingClass;
        }

        [StructLayout(LayoutKind.Sequential)]
        struct IoCounters
        {
            public ulong A, B, C, D, E, F;
        }

        [StructLayout(LayoutKind.Sequential)]
        struct ExtendedLimits
        {
            public BasicLimits Basic;
            public IoCounters Io;
            public UIntPtr ProcessMemoryLimit;
            public UIntPtr JobMemoryLimit;
            public UIntPtr PeakProcessMemoryUsed;
            public UIntPtr PeakJobMemoryUsed;
        }

        delegate bool EachWindow(IntPtr window, IntPtr extra);

        [DllImport("ole32.dll")]
        static extern int GetRunningObjectTable(int reserved, out IRunningObjectTable table);
        [DllImport("ole32.dll")]
        static extern int CreateBindCtx(int reserved, out IBindCtx context);
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
        static extern IntPtr CreateJobObject(IntPtr attributes, string name);
        [DllImport("kernel32.dll")]
        static extern bool SetInformationJobObject(IntPtr job, int kind, IntPtr info, uint size);
        [DllImport("kernel32.dll")]
        static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);
        [DllImport("kernel32.dll")]
        static extern bool IsProcessInJob(IntPtr process, IntPtr job, out bool result);
        [DllImport("kernel32.dll")]
        static extern IntPtr OpenProcess(uint access, bool inherit, uint id);
        [DllImport("kernel32.dll")]
        static extern bool CloseHandle(IntPtr handle);
        [DllImport("user32.dll")]
        static extern bool EnumWindows(EachWindow each, IntPtr extra);
        [DllImport("user32.dll")]
        static extern uint GetWindowThreadProcessId(IntPtr window, out uint id);
        [DllImport("user32.dll")]
        static extern bool IsWindowVisible(IntPtr window);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        static extern int GetClassName(IntPtr window, StringBuilder text, int size);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        static extern int GetWindowText(IntPtr window, StringBuilder text, int size);

        static IntPtr job = IntPtr.Zero;
        static Process process;

        // Группа процессов, которую Windows закрывает целиком, когда закрыт
        // последний её описатель, — то есть когда не стало рабочего процесса.
        static bool Fence()
        {
            const int ExtendedLimitInformation = 9;
            const uint KillOnJobClose = 0x2000;
            job = CreateJobObject(IntPtr.Zero, null);
            if (job == IntPtr.Zero) return false;
            ExtendedLimits limits = new ExtendedLimits();
            limits.Basic.LimitFlags = KillOnJobClose;
            int size = Marshal.SizeOf(typeof(ExtendedLimits));
            IntPtr memory = Marshal.AllocHGlobal(size);
            try
            {
                Marshal.StructureToPtr(limits, memory, false);
                return SetInformationJobObject(job, ExtendedLimitInformation, memory, (uint)size);
            }
            finally
            {
                Marshal.FreeHGlobal(memory);
            }
        }

        // Запустить свой SolidWorks без окна и заставки. Ответ — номер
        // процесса; fenced — попал ли он в группу процессов.
        public static int Start(string exe, out bool fenced)
        {
            bool ready = Fence();
            ProcessStartInfo info = new ProcessStartInfo(exe, "/r");
            info.UseShellExecute = false;
            info.WindowStyle = ProcessWindowStyle.Hidden;
            process = Process.Start(info);
            fenced = ready && AssignProcessToJobObject(job, process.Handle);
            return process.Id;
        }

        public static bool Exited()
        {
            return process == null || process.HasExited;
        }

        // Время старта своего процесса: вместе с номером отличает его от
        // чужого процесса, которому Windows позже дала тот же номер.
        public static long Started()
        {
            return process.StartTime.ToFileTimeUtc();
        }

        public static bool WaitExit(int milliseconds)
        {
            return process == null || process.WaitForExit(milliseconds);
        }

        public static void Kill()
        {
            try
            {
                if (process != null && !process.HasExited) process.Kill();
            }
            catch (Exception) { }
        }

        // COM-объект своего SolidWorks из ROT. null — ещё не записался.
        public static object Find(int id)
        {
            IRunningObjectTable table;
            IBindCtx context;
            if (GetRunningObjectTable(0, out table) != 0) return null;
            if (CreateBindCtx(0, out context) != 0) return null;
            IEnumMoniker all;
            table.EnumRunning(out all);
            IMoniker[] one = new IMoniker[1];
            string wanted = "SolidWorks_PID_" + id;
            while (all.Next(1, one, IntPtr.Zero) == 0)
            {
                string name;
                try
                {
                    one[0].GetDisplayName(context, null, out name);
                }
                catch (Exception)
                {
                    continue;
                }
                if (string.Equals(name, wanted, StringComparison.OrdinalIgnoreCase))
                {
                    object found;
                    table.GetObject(one[0], out found);
                    return found;
                }
            }
            return null;
        }

        // Наш ли это процесс: сам SolidWorks или запущенный им (отчёт об
        // ошибке) — тот, что в нашей группе процессов.
        static bool Mine(uint id)
        {
            if (process != null && id == (uint)process.Id) return true;
            if (job == IntPtr.Zero) return false;
            const uint QueryLimited = 0x1000;
            IntPtr handle = OpenProcess(QueryLimited, false, id);
            if (handle == IntPtr.Zero) return false;
            bool inside;
            bool known = IsProcessInJob(handle, job, out inside);
            CloseHandle(handle);
            return known && inside;
        }

        // Заголовок видимого окна-сообщения своего SolidWorks. Свой
        // SolidWorks невидим: любое такое окно — он чего-то ждёт от человека
        // (или это отчёт об ошибке после вылета).
        static string Dialog()
        {
            string title = "";
            EnumWindows(delegate (IntPtr window, IntPtr extra)
            {
                if (!IsWindowVisible(window)) return true;
                uint id;
                GetWindowThreadProcessId(window, out id);
                if (!Mine(id)) return true;
                StringBuilder text = new StringBuilder(256);
                GetClassName(window, text, text.Capacity);
                if (text.ToString() != "#32770") return true;
                text.Clear();
                GetWindowText(window, text, text.Capacity);
                title = text.Length == 0 ? "?" : text.ToString().Replace("\t", " ");
                return false;
            }, IntPtr.Zero);
            return title;
        }

        // Сторожевые нити. Пульс раз в секунду: BEAT, время процессора в
        // миллисекундах, заголовок окна-сообщения; GONE — процесса нет.
        public static void Watch(int parent)
        {
            Thread pulse = new Thread(delegate ()
            {
                while (true)
                {
                    Thread.Sleep(1000);
                    try
                    {
                        if (process == null) continue;
                        if (process.HasExited)
                        {
                            Wire.Say("GONE");
                            continue;
                        }
                        process.Refresh();
                        long spent = (long)process.TotalProcessorTime.TotalMilliseconds;
                        Wire.Say("BEAT\t" + spent + "\t" + Dialog());
                    }
                    catch (Exception) { }
                }
            });
            pulse.IsBackground = true;
            pulse.Start();

            Thread owner = new Thread(delegate ()
            {
                try
                {
                    Process.GetProcessById(parent).WaitForExit();
                }
                catch (Exception) { }
                // Nonius не стало. Обычно он просто закончил работу: тогда
                // рабочий процесс уже увидел конец stdin и сам вежливо
                // закрывает SolidWorks. Даём на это время, потом снимаем.
                Thread.Sleep(20000);
                Kill();
                Environment.Exit(3);
            });
            owner.IsBackground = true;
            owner.Start();
        }
    }
}
