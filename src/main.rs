extern crate clap;
extern crate chrono;

use core::ffi::c_char;
use std::ffi::CString;
use ni_daqmx_sys;

use clap::Parser;
use clap::{Arg, ArgMatches, ValueEnum};

use std::time::{SystemTime};

use chrono::prelude::*;


static SAMPLES_PER_SECOND : ni_daqmx_sys::float64 = 1000.0;
static SAMPLES: i32 = 1000;
static CHANNELS: i32 = 2;


#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
#[derive(Debug)]
enum MeasurementMode {
    /// Referenced single-ended mode
    RSE,
    /// Non-referenced single-ended mode
    NRSE,
    /// Differential mode
    DIFF,
    /// Pseudodifferential mode
    PSEUDODIFF
}

/// VeSys XML project post-processor 
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The names of the physical channels to use to create virtual channels. You can specify a list or range of physical channels.
    ///
    /// SYNTAX: <device>/<channel>, <device>/<channel>, ...
    ///
    /// EXAMPLE: cDAQ9181-1FE3677Mod1/ai0, cDAQ9181-1FE3677Mod1/ai8
    channels: String,
    #[arg(value_enum, default_value_t = MeasurementMode::RSE)]
    /// Terminal configuration mode
    mode: MeasurementMode,
    /// Sample rate [samples/sec]
    #[arg(short, long, default_value_t = 1000.0)]
    rate: f64,
    /// Number of samples to take for each measurement batch [N]
    #[arg(short, long, default_value_t = 1000)]
    size: u64,
}

macro_rules! check_err {
    ($prefix:expr,$err:expr) => {
        if $err != 0 {
            eprintln!("{} error: {:?}", $prefix, $err);
        }
    };
}

macro_rules! return_if_err {
    ($prefix:expr,$err:expr) => {
        if $err != 0 {
            eprintln!("{} error: {:?}", $prefix, $err);
            return Err($err);
        }
    };
}


#[derive(Debug)]
struct DAQVTask {
    task_handle : ni_daqmx_sys::TaskHandle,
    data : Vec<ni_daqmx_sys::float64>,
    pub channels : usize
}

impl DAQVTask {
    fn new(channels : &str, mode : MeasurementMode, sample_rate : ni_daqmx_sys::float64, sample_count : u64) -> Result<DAQVTask, i32> {
        let mut task_handle : ni_daqmx_sys::TaskHandle = std::ptr::null_mut();
        unsafe {
            // Create measurement task
            return_if_err!("DAQmxCreateTask", ni_daqmx_sys::DAQmxCreateTask(std::ptr::null(), &mut task_handle));

            // Translate mode options
            let mode = match mode {
                MeasurementMode::RSE => ni_daqmx_sys::DAQmx_Val_RSE,
                MeasurementMode::NRSE => ni_daqmx_sys::DAQmx_Val_NRSE,
                MeasurementMode::DIFF => ni_daqmx_sys::DAQmx_Val_Diff,
                MeasurementMode::PSEUDODIFF => ni_daqmx_sys::DAQmx_Val_PseudoDiff,
            };

            let ch_name = CString::new(channels).expect("CString::new failed");
            let ch_name_ptr: *const c_char = ch_name.as_ptr();
        
            // Create channels and set measurement mode
            return_if_err!("DAQmxCreateAIVoltageChan", ni_daqmx_sys::DAQmxCreateAIVoltageChan(task_handle, ch_name_ptr, std::ptr::null(), mode, -10.0, 10.0, ni_daqmx_sys::DAQmx_Val_Volts, std::ptr::null()));
        }
            // Find number of channels created
            let mut channels : u32 = 0;
        unsafe {
            return_if_err!("DAQmxGetTaskNumChans", ni_daqmx_sys::DAQmxGetTaskNumChans(task_handle, &mut channels));
        }
            assert!(channels > 0);

        unsafe {
            // Set sample rate, sample count, trigger mode
            return_if_err!("DAQmxCfgSampClkTiming", ni_daqmx_sys::DAQmxCfgSampClkTiming(task_handle, std::ptr::null(), sample_rate, ni_daqmx_sys::DAQmx_Val_Rising, ni_daqmx_sys::DAQmx_Val_FiniteSamps, sample_count));
        }

        let mut data = Vec::<ni_daqmx_sys::float64>::new();
        data.resize((channels as usize)*(sample_count as usize), 0.0);

        Ok(DAQVTask {
            task_handle : task_handle,
            data : data, // data buffer
            channels : channels.try_into().unwrap()
        })
    }

    /// Read samples
    fn read_samples(&mut self) -> Result<&[ni_daqmx_sys::float64], i32> {
        let mut read : i32 = -1;

        unsafe {
            // Start
            return_if_err!("DAQmxStartTask", ni_daqmx_sys::DAQmxStartTask(self.task_handle));
            // Read
            return_if_err!("DAQmxReadAnalogF64", 
                ni_daqmx_sys::DAQmxReadAnalogF64(
                    self.task_handle, 
                    ni_daqmx_sys::DAQmx_Val_Auto, 
                    10.0, 
                    ni_daqmx_sys::DAQmx_Val_GroupByScanNumber as u32, 
                    self.data.as_mut_ptr(), 
                    self.data.len() as u32, 
                    &mut read, std::ptr::null_mut()));

            // Stop
            return_if_err!("DAQmxStopTask", ni_daqmx_sys::DAQmxStopTask(self.task_handle))
        }

        // return slice to buffer in case not all samples were read
        return Ok(&self.data[0..read.try_into().unwrap_or(0)]);
    }

}

impl Drop for DAQVTask {
    /// Clean up
    fn drop(&mut self) {

        if self.task_handle != std::ptr::null_mut() {
            unsafe {
                let err = ni_daqmx_sys::DAQmxStopTask(self.task_handle);
                check_err!("DAQmxStopTask", err);
                let err = ni_daqmx_sys::DAQmxClearTask(self.task_handle);
                check_err!("DAQmxClearTask", err);
            }
        }
    }
}

fn main() {
    let args = Args::parse();

    let mut daqmx = DAQVTask::new(&args.channels, MeasurementMode::RSE, args.rate, args.size);
    loop {
    match daqmx {
        Ok(ref mut task) => {
            let channels = task.channels;
            match task.read_samples() {
                Ok(samples) => {
                    for row in 0..samples.len()/channels {
                        let row_offset = row*channels;
                        let time = Local::now();
                        print!("{:?}", time.format("%Y-%m-%d %H:%M:%S").to_string());
                        for column in 0..channels {
                            //if column > 0 { print!(",") };
                            print!(", {}", samples[row_offset + column]);
                        }
                        print!("\n");
                    }
                }
                Err(code) => {
                    eprintln!("One of NI-DAQmx API calls returned an error code: {}", code);        
                }
            }
        } 
        Err(code) => {
            eprintln!("One of NI-DAQmx API calls returned an error code: {}", code);
            return;
        }
    }

    }


    return;
    unsafe {
        let mut task_handle : ni_daqmx_sys::TaskHandle = std::ptr::null_mut();
        //let ch : c_str
        //let task_name: *const c_char = CString::new("daq01").expect("CString::new failed").as_ptr();

        let err = ni_daqmx_sys::DAQmxCreateTask(std::ptr::null(), &mut task_handle);
        check_err!("DAQmxCreateTask", err);
        
        

        let ch_name = CString::new("cDAQ9181-1FE3677Mod1/ai0, cDAQ9181-1FE3677Mod1/ai8").expect("CString::new failed");
        let ch_name_ptr: *const c_char = ch_name.as_ptr();
        let err = ni_daqmx_sys::DAQmxCreateAIVoltageChan(task_handle, ch_name_ptr, std::ptr::null(), ni_daqmx_sys::DAQmx_Val_RSE, -10.0, 10.0, ni_daqmx_sys::DAQmx_Val_Volts, std::ptr::null());
        check_err!("DAQmxCreateAIVoltageChan", err);

        let mut channels : u32 = 0;
        let err = ni_daqmx_sys::DAQmxGetTaskNumChans(task_handle, &mut channels);
        check_err!("DAQmxGetTaskNumChans", err);
        println!("Channels {}", channels);

        let err = ni_daqmx_sys::DAQmxCfgSampClkTiming(task_handle, std::ptr::null(), SAMPLES_PER_SECOND, ni_daqmx_sys::DAQmx_Val_Rising, ni_daqmx_sys::DAQmx_Val_FiniteSamps, 1000);
        check_err!("DAQmxCfgSampClkTiming", err);
        let err = ni_daqmx_sys::DAQmxStartTask(task_handle);
        check_err!("DAQmxStartTask", err);
        let mut data : [ni_daqmx_sys::float64; (CHANNELS*SAMPLES) as usize] = [0.0; (CHANNELS*SAMPLES) as usize];
        let data_ptr: *mut f64 = data.as_mut_ptr();
        let mut read : i32 = -1;
        let err = ni_daqmx_sys::DAQmxReadAnalogF64(task_handle, SAMPLES, 10.0, ni_daqmx_sys::DAQmx_Val_GroupByScanNumber as u32, data_ptr, (CHANNELS*SAMPLES) as u32, &mut read, std::ptr::null_mut());
        

        check_err!("DAQmxReadAnalogF64", err);
        //println!("DAQmxReadAnalogF64 {:?}", data);
        let err = ni_daqmx_sys::DAQmxStopTask(task_handle);
        check_err!("DAQmxStopTask", err);
        println!("{}", read);

        // for i in 0..data.len() {
        //     println!("{}", data[i]);
        // }

        for i in 0..data.len()/2 {
            let j = i*2;
            println!("{} {}", data[j], data[j+1]);
        }

    }
    
}
