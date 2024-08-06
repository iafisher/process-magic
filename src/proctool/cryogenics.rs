use anyhow::{anyhow, Result};
use nix::{sys, unistd};
use serde::{Deserialize, Serialize};

use crate::{
    proctool::{pcontroller::ProcessController, terminals},
    teleclient::myprocfs,
};

#[derive(Serialize, Deserialize)]
pub struct ProcessState {
    pub memory_maps: Vec<myprocfs::MemoryMap>,
    // from libc::user_regs_struct
    // very much ARM64 specific
    pub regs: [u64; 31],
    pub sp: u64,
    pub pc: u64,
    pub pstate: u64,
}

pub fn freeze(pid: unistd::Pid) -> Result<ProcessState> {
    let controller = ProcessController::new(pid);
    controller.attach()?;
    let registers = controller.get_registers()?;
    controller.detach_and_stop()?;

    let mut memory_maps = myprocfs::read_memory_maps(pid.as_raw())?;
    println!("reading process memory... this may take a while");
    myprocfs::populate_memory(pid, &mut memory_maps)?;

    Ok(ProcessState {
        memory_maps,
        regs: registers.regs,
        sp: registers.sp,
        pc: registers.pc,
        pstate: registers.pstate,
    })
}

pub fn thaw(state: &ProcessState) -> Result<()> {
    match unsafe { unistd::fork() }? {
        unistd::ForkResult::Parent { child } => {
            println!("child pid: {}", child);
            sys::wait::waitpid(child, Some(sys::wait::WaitPidFlag::WSTOPPED))
                .map_err(|e| anyhow!("failed to waitpid: {}", e))?;

            let controller = ProcessController::new(child);
            let svc_region_addr = controller
                .map_svc_region()
                .map_err(|e| anyhow!("failed to map svc region: {}", e))?;

            for map in state.memory_maps.iter() {
                println!(
                    "mapping memory region at {:#x} (size={})",
                    map.base_address, map.size
                );
                if let Err(e) = controller
                    .map_and_fill_region(svc_region_addr, map)
                    .map_err(|e| {
                        anyhow!(
                            "failed to map/fill memory region (addr={:#x}): {}",
                            map.base_address,
                            e
                        )
                    })
                {
                    println!("error: {}", e);
                }
            }

            controller.set_registers(libc::user_regs_struct {
                regs: state.regs,
                sp: state.sp,
                pc: state.pc,
                pstate: state.pstate,
            })?;

            // TODO:
            terminals::clear_terminal("/dev/tty")?;
            controller.detach()?;
            // controller.detach_and_stop()?;
            controller.waitpid()?;
        }
        unistd::ForkResult::Child => {
            sys::ptrace::traceme()?;
            sys::signal::raise(sys::signal::SIGSTOP)?;
        }
    }

    Ok(())
}
