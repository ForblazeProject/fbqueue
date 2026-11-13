using System;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

// Test infrastructure only. Does not change FBQueue's Windows process model.
namespace FBQWindowsTest {
    public static class Native {
        [StructLayout(LayoutKind.Sequential)] internal struct SECURITY_ATTRIBUTES {
            internal int length; internal IntPtr descriptor; [MarshalAs(UnmanagedType.Bool)] internal bool inherit;
        }
        [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] internal struct STARTUPINFO {
            internal int cb; internal string reserved, desktop, title;
            internal uint x,y,xsize,ysize,xchars,ychars,fill,flags;
            internal ushort show,reserved2; internal IntPtr reservedPtr, stdin,stdout,stderr;
        }
        [StructLayout(LayoutKind.Sequential)] internal struct STARTUPINFOEX {
            internal STARTUPINFO info; internal IntPtr attributes;
        }
        [StructLayout(LayoutKind.Sequential)] internal struct PROCESS_INFORMATION {
            internal IntPtr process, thread; internal uint pid, tid;
        }
        [StructLayout(LayoutKind.Sequential)] internal struct BASIC_LIMIT {
            internal long processTime,jobTime; internal uint flags;
            internal UIntPtr minWorking,maxWorking; internal uint activeLimit;
            internal UIntPtr affinity; internal uint priority,scheduling;
        }
        [StructLayout(LayoutKind.Sequential)] internal struct IO_COUNTERS {
            internal ulong readOps,writeOps,otherOps,readBytes,writeBytes,otherBytes;
        }
        [StructLayout(LayoutKind.Sequential)] internal struct EXTENDED_LIMIT {
            internal BASIC_LIMIT basic; internal IO_COUNTERS io;
            internal UIntPtr processMemory,jobMemory,peakProcessMemory,peakJobMemory;
        }
        [StructLayout(LayoutKind.Sequential)] internal struct ACCOUNTING {
            internal long totalUser,totalKernel,periodUser,periodKernel;
            internal uint pageFaults,totalProcesses,activeProcesses,terminatedProcesses;
        }
        [DllImport("kernel32.dll", SetLastError=true, CharSet=CharSet.Unicode)]
        internal static extern IntPtr CreateJobObjectW(IntPtr attributes,string name);
        [DllImport("kernel32.dll", SetLastError=true)]
        internal static extern bool SetInformationJobObject(IntPtr job,int kind,ref EXTENDED_LIMIT info,uint length);
        [DllImport("kernel32.dll", SetLastError=true)]
        internal static extern bool QueryInformationJobObject(IntPtr job,int kind,out ACCOUNTING info,uint length,IntPtr returned);
        [DllImport("kernel32.dll", SetLastError=true)] internal static extern bool TerminateJobObject(IntPtr job,uint code);
        [DllImport("kernel32.dll", SetLastError=true)] internal static extern bool CloseHandle(IntPtr handle);
        [DllImport("kernel32.dll", SetLastError=true)] internal static extern bool InitializeProcThreadAttributeList(IntPtr list,int count,int flags,ref IntPtr size);
        [DllImport("kernel32.dll", SetLastError=true)] internal static extern bool UpdateProcThreadAttribute(IntPtr list,uint flags,IntPtr kind,IntPtr value,IntPtr size,IntPtr previous,IntPtr returned);
        [DllImport("kernel32.dll")] internal static extern void DeleteProcThreadAttributeList(IntPtr list);
        [DllImport("kernel32.dll", SetLastError=true, CharSet=CharSet.Unicode)] internal static extern IntPtr CreateFileW(string path,uint access,uint share,ref SECURITY_ATTRIBUTES attributes,uint creation,uint flags,IntPtr template);
        [DllImport("kernel32.dll", SetLastError=true, CharSet=CharSet.Unicode)] internal static extern bool CreateProcessW(string application,StringBuilder command,IntPtr processAttributes,IntPtr threadAttributes,bool inherit,uint flags,IntPtr environment,string directory,ref STARTUPINFOEX startup,out PROCESS_INFORMATION info);
        [DllImport("kernel32.dll", SetLastError=true)] internal static extern uint ResumeThread(IntPtr thread);
        [DllImport("kernel32.dll", SetLastError=true)] internal static extern uint WaitForSingleObject(IntPtr handle,uint millis);
        [DllImport("kernel32.dll", SetLastError=true)] internal static extern bool GetExitCodeProcess(IntPtr process,out uint code);
        [DllImport("kernel32.dll", SetLastError=true)] internal static extern bool TerminateProcess(IntPtr process,uint code);
        [DllImport("kernel32.dll", SetLastError=true)] internal static extern bool IsProcessInJob(IntPtr process,IntPtr job,out bool present);
        [DllImport("kernel32.dll")] internal static extern IntPtr GetCurrentProcess();

        internal static Exception Error(string action) {
            int code=Marshal.GetLastWin32Error();
            return new Win32Exception(code, action+" (Win32 "+code+"): "+new Win32Exception(code).Message);
        }
        public static string Quote(string value) {
            StringBuilder b=new StringBuilder("\""); int slashes=0;
            foreach(char c in value) {
                if(c=='\\') { slashes++; continue; }
                if(c=='"') { b.Append('\\',slashes*2+1); b.Append(c); }
                else { b.Append('\\',slashes); b.Append(c); }
                slashes=0;
            }
            b.Append('\\',slashes*2); b.Append('"'); return b.ToString();
        }
        public static bool InsideJob() {
            bool present; if(!IsProcessInJob(GetCurrentProcess(),IntPtr.Zero,out present)) throw Error("IsProcessInJob"); return present;
        }
        public static string ReadShared(string path) {
            if(!File.Exists(path)) return "";
            using(FileStream f=new FileStream(path,FileMode.Open,FileAccess.Read,FileShare.ReadWrite|FileShare.Delete))
            using(StreamReader r=new StreamReader(f,Encoding.UTF8,true)) return r.ReadToEnd();
        }
        internal static int Execute(string exe,string[] args,string cwd,string output,string error,int timeout,IntPtr job) {
            if(Environment.OSVersion.Platform!=PlatformID.Win32NT) throw new PlatformNotSupportedException("Windows native runner required");
            if(timeout<=0) throw new ArgumentOutOfRangeException("timeout");
            SECURITY_ATTRIBUTES sa=new SECURITY_ATTRIBUTES(); sa.length=Marshal.SizeOf(typeof(SECURITY_ATTRIBUTES)); sa.inherit=true;
            IntPtr stdout=IntPtr.Zero,stderr=IntPtr.Zero,stdin=IntPtr.Zero,list=IntPtr.Zero,handles=IntPtr.Zero,jobs=IntPtr.Zero;
            PROCESS_INFORMATION pi=new PROCESS_INFORMATION(); bool initialized=false;
            try {
                stdout=CreateFileW(output,0x40000000,7,ref sa,2,0x80,IntPtr.Zero);
                if(stdout==new IntPtr(-1)) throw Error("Create stdout file");
                stderr=CreateFileW(error,0x40000000,7,ref sa,2,0x80,IntPtr.Zero);
                if(stderr==new IntPtr(-1)) throw Error("Create stderr file");
                stdin=CreateFileW("NUL",0x80000000,7,ref sa,3,0x80,IntPtr.Zero);
                if(stdin==new IntPtr(-1)) throw Error("Open null stdin");
                IntPtr size=IntPtr.Zero; int count=job==IntPtr.Zero?1:2;
                InitializeProcThreadAttributeList(IntPtr.Zero,count,0,ref size);
                list=Marshal.AllocHGlobal(size);
                if(!InitializeProcThreadAttributeList(list,count,0,ref size)) throw Error("Initialize attributes");
                initialized=true;
                handles=Marshal.AllocHGlobal(IntPtr.Size*3);
                Marshal.WriteIntPtr(handles,0,stdin); Marshal.WriteIntPtr(handles,IntPtr.Size,stdout); Marshal.WriteIntPtr(handles,IntPtr.Size*2,stderr);
                if(!UpdateProcThreadAttribute(list,0,new IntPtr(0x20002),handles,new IntPtr(IntPtr.Size*3),IntPtr.Zero,IntPtr.Zero)) throw Error("Restrict inherited handles");
                if(job!=IntPtr.Zero) {
                    jobs=Marshal.AllocHGlobal(IntPtr.Size); Marshal.WriteIntPtr(jobs,job);
                    // JOB_LIST assigns atomically at process creation: no unowned suspended-process gap.
                    if(!UpdateProcThreadAttribute(list,0,new IntPtr(0x2000D),jobs,new IntPtr(IntPtr.Size),IntPtr.Zero,IntPtr.Zero)) throw Error("Attach job at creation");
                }
                STARTUPINFOEX si=new STARTUPINFOEX(); si.info.cb=Marshal.SizeOf(typeof(STARTUPINFOEX)); si.attributes=list;
                si.info.flags=0x100; si.info.stdin=stdin; si.info.stdout=stdout; si.info.stderr=stderr;
                StringBuilder command=new StringBuilder(Quote(exe)); foreach(string arg in args) command.Append(" ").Append(Quote(arg));
                if(!CreateProcessW(exe,command,IntPtr.Zero,IntPtr.Zero,true,0x08080004,IntPtr.Zero,cwd,ref si,out pi)) throw Error("CreateProcessW");
                bool present;
                if(!IsProcessInJob(pi.process,job,out present)) throw Error("Verify child job membership");
                if(!present) throw new InvalidOperationException("Child is not contained in the required job");
                if(ResumeThread(pi.thread)==0xffffffff) throw Error("Resume child");
                uint wait=WaitForSingleObject(pi.process,(uint)timeout);
                if(wait==0x102) throw new TimeoutException("Command exceeded "+timeout+" ms: "+exe);
                if(wait!=0) throw Error("Wait for child");
                uint exit; if(!GetExitCodeProcess(pi.process,out exit)) throw Error("Read exit code"); return unchecked((int)exit);
            } finally {
                if(pi.process!=IntPtr.Zero) {
                    if(WaitForSingleObject(pi.process,0)==0x102) { TerminateProcess(pi.process,124); WaitForSingleObject(pi.process,5000); }
                    CloseHandle(pi.process);
                }
                if(pi.thread!=IntPtr.Zero) CloseHandle(pi.thread);
                if(initialized) DeleteProcThreadAttributeList(list);
                if(list!=IntPtr.Zero) Marshal.FreeHGlobal(list);
                if(handles!=IntPtr.Zero) Marshal.FreeHGlobal(handles);
                if(jobs!=IntPtr.Zero) Marshal.FreeHGlobal(jobs);
                foreach(IntPtr handle in new IntPtr[]{stdout,stderr,stdin}) if(handle!=IntPtr.Zero&&handle!=new IntPtr(-1)) CloseHandle(handle);
            }
        }
        public static int RunInherited(string exe,string[] args,string cwd,string output,string error,int timeout) {
            if(!InsideJob()) throw new InvalidOperationException("Refusing an uncontained test command");
            return Execute(exe,args,cwd,output,error,timeout,IntPtr.Zero);
        }
    }
    public sealed class Job : IDisposable {
        private IntPtr handle;
        public Job() {
            handle=Native.CreateJobObjectW(IntPtr.Zero,null); if(handle==IntPtr.Zero) throw Native.Error("CreateJobObjectW");
            Native.EXTENDED_LIMIT limits=new Native.EXTENDED_LIMIT(); limits.basic.flags=0x2000;
            if(!Native.SetInformationJobObject(handle,9,ref limits,(uint)Marshal.SizeOf(typeof(Native.EXTENDED_LIMIT)))) {
                Exception error=Native.Error("Set kill-on-close limit"); Dispose(); throw error;
            }
        }
        public int Run(string exe,string[] args,string cwd,string output,string error,int timeout) {
            if(handle==IntPtr.Zero) throw new ObjectDisposedException("Job"); return Native.Execute(exe,args,cwd,output,error,timeout,handle);
        }
        public uint ActiveProcesses {
            get {
                Native.ACCOUNTING info;
                if(!Native.QueryInformationJobObject(handle,1,out info,(uint)Marshal.SizeOf(typeof(Native.ACCOUNTING)),IntPtr.Zero)) throw Native.Error("Read job accounting");
                return info.activeProcesses;
            }
        }
        public void StopAndVerify() {
            if(handle==IntPtr.Zero) return;
            if(!Native.TerminateJobObject(handle,125)) throw Native.Error("Terminate owned job");
            for(int i=0;i<100;i++) { if(ActiveProcesses==0) return; Thread.Sleep(50); }
            throw new TimeoutException("Owned job still has active processes after cleanup");
        }
        public void Dispose() { if(handle!=IntPtr.Zero) { Native.CloseHandle(handle); handle=IntPtr.Zero; } GC.SuppressFinalize(this); }
        ~Job() { Dispose(); }
    }
}
