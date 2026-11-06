#include <libusb.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
static libusb_device_handle *h;
static void drain(void){unsigned char t[64];int n;while(!libusb_bulk_transfer(h,0x82,t,64,&n,30)&&n>0);}
static int out(const char*s){int n;return libusb_bulk_transfer(h,0x01,(unsigned char*)s,strlen(s),&n,1000);}
int main(int c,char**v){
 libusb_init(0); h=libusb_open_device_with_vid_pid(0,0x03eb,0x6124);
 if(!h){puts("no device");return 1;}
 libusb_detach_kernel_driver(h,1); if(libusb_claim_interface(h,1)){puts("claim failed");return 1;}
 out("N#"); drain();
 for(int i=1;i<c;i++){ char s[48]; unsigned char b[64]; int n;
  if(v[i][0]=='r'){ unsigned a=strtoul(v[i]+1,0,16); snprintf(s,sizeof s,"w%08X,4#",a); drain(); out(s);
    n=0; libusb_bulk_transfer(h,0x82,b,64,&n,1000); printf("[%08X] = %08X\n",a,n>=4?b[3]<<24|b[2]<<16|b[1]<<8|b[0]:0xDEADDEAD);}
  else if(v[i][0]=='w'){ unsigned a=strtoul(v[i]+1,0,16); char*e=strchr(v[i],'='); unsigned val=strtoul(e+1,0,16); snprintf(s,sizeof s,"W%08X,%08X#",a,val); out(s); printf("[%08X] <- %08X\n",a,val); usleep(2000);}
  else if(v[i][0]=='s') usleep(atoi(v[i]+1)*1000);
 } return 0;}
