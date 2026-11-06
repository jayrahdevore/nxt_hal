#include <libusb.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static libusb_device_handle *h;
static int xfer_out(const char*s){int n;return libusb_bulk_transfer(h,0x01,(unsigned char*)s,strlen(s),&n,1000);}
static void drain(void){unsigned char t[64];int n;while(!libusb_bulk_transfer(h,0x82,t,64,&n,50)&&n>0);}
static int rd(unsigned char*b,int len){int n=0;int r=libusb_bulk_transfer(h,0x82,b,len,&n,1000);return r?-1:n;}
int main(int argc,char**argv){
 libusb_init(0);
 h=libusb_open_device_with_vid_pid(0,0x03eb,0x6124);
 if(!h){puts("no device");return 1;}
 libusb_detach_kernel_driver(h,1);
 if(libusb_claim_interface(h,1)){puts("claim failed");return 1;}
 xfer_out("N#"); unsigned char b[64]; rd(b,64);
 for(int i=1;i+1<argc;i+=2){
  unsigned a=strtoul(argv[i],0,16), nw=strtoul(argv[i+1],0,0);
  for(unsigned k=0;k<nw;k++){
   char c[40]; snprintf(c,sizeof c,"w%08X,4#",a+4*k);
   drain(); xfer_out(c); int n=rd(b,64);
   if(n<4){printf("%08X: read fail (%d)\n",a+4*k,n);continue;}
   printf("%08X: %02X%02X%02X%02X\n",a+4*k,b[3],b[2],b[1],b[0]);
  }}
 return 0;}
