extern crate clap;
extern crate chrono;

use chrono::TimeDelta;
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
    samples : Vec<ni_daqmx_sys::float64>,
    timestamps : Vec<DateTime<Local>>,
    channels : usize,
    sample_rate : ni_daqmx_sys::float64
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

        let mut samples = Vec::<ni_daqmx_sys::float64>::new();
        let buffer_size = (channels as usize)*(sample_count as usize);
        samples.resize(buffer_size, 0.0);

        let mut timestamps = Vec::<DateTime<Local>>::new();
        timestamps.resize(buffer_size, Local::now());

        Ok(DAQVTask {
            task_handle : task_handle,
            samples : samples, // data buffer
            timestamps : timestamps,
            sample_rate : sample_rate,
            channels : channels.try_into().unwrap()
        })
    }

    /// Read samples, returns number of sampes read
    fn acquire_samples(&mut self) -> Result<i32, i32> {
        let mut read : i32 = -1;

        let start_time = Local::now();

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
                    self.samples.as_mut_ptr(), 
                    self.samples.len() as u32, 
                    &mut read, std::ptr::null_mut()));

            // Stop
            return_if_err!("DAQmxStopTask", ni_daqmx_sys::DAQmxStopTask(self.task_handle))
        }

        // Fill timestamps
        let period = TimeDelta::nanoseconds((1e9*(1.0/self.sample_rate)) as i64);
        let p = start_time + period*2;
        for i in 0..read {
            let timestamp = start_time + period*i;
            let i : usize = i.try_into().unwrap();
            self.timestamps[i] = timestamp;
        }

        self.samples_read = read;

        return read;
    }

    /// Get read samples from the buffer
    fn get_samples(self) -> Result<&[ni_daqmx_sys::float64], i32> {
        // return slice to buffer in case not all samples were read
        return Ok(&self.samples[0..read.try_into().unwrap()]);
    }

    fn get_timestamps(self) -> Result<&[ni_daqmx_sys::float64], i32> {
        // return slice to buffer in case not all samples were read
        return Ok(&self.timestamps[0..read.try_into().unwrap()]);
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

    let s = 0.5;
    let msf:f64 = (1000.0*s);
    let msu:u32 = msf.floor() as u32;
    println!("{}", msu);
    let args = Args::parse();
    return;

    let mut daqmx = DAQVTask::new(&args.channels, MeasurementMode::RSE, args.rate, args.size);
    loop {
    match daqmx {
        Ok(ref mut task) => {
            let channels = task.channels;
            task.channels = 1;
            // mark start time
            match task.read_samples() {
                Ok(samples) => {
                    for row in 0..samples.len()/channels {
                        let row_offset = row*channels;
                        let time = Local::now();
                        print!("{:?}", time.format("%Y-%m-%d %H:%M:%S.%3f").to_string());
                        //print!("{:?}", time.format("%s").to_string());
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
